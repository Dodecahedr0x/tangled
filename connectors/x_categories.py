"""Independent read-only X categories; permission failures do not hide other data."""
import copy
import hashlib
import json
import os
from common import ConnectorError, batch

CATEGORY_SCOPES = {'posts': 'tweet.read', 'dms': 'dm.read', 'likes': 'like.read',
                   'bookmarks': 'bookmark.read', 'followers': 'follows.read',
                   'following': 'follows.read', 'profile': 'users.read'}
POST_FIELDS = 'id,text,created_at,conversation_id,attachments,entities,note_post,edit_controls'
USER_FIELDS = 'id,name,username,created_at,description,location,entities,profile_image_url,protected,public_metrics,url,verified'
MEDIA_FIELDS = 'media_key,type,url,preview_image_url,variants,alt_text,duration_ms,height,width'


class XSourceError(ConnectorError):
    def __init__(self, status=None, message=None):
        self.status = status
        super().__init__(message or (f'X rejected source request (HTTP {status})' if status else 'X source unavailable after 5 attempts'))


def selected_categories(value=None):
    value = os.environ.get('X_CATEGORIES', '') if value is None else value
    categories = list(dict.fromkeys(item.strip() for item in value.split(',') if item.strip())) if value else list(CATEGORY_SCOPES)
    if not categories or any(item not in CATEGORY_SCOPES for item in categories):
        raise ConnectorError('X_CATEGORIES must contain posts,dms,likes,bookmarks,followers,following,profile')
    return categories


def requested_scopes(categories=None):
    scopes = ['tweet.read', 'users.read', 'offline.access']
    for category in categories or selected_categories():
        scope = CATEGORY_SCOPES[category]
        if scope not in scopes:
            scopes.append(scope)
    return ' '.join(scopes)


def category_request(account, category, cursor):
    paths = {'posts': f'/users/{account}/tweets', 'dms': '/dm_events',
             'likes': f'/users/{account}/liked_tweets', 'bookmarks': f'/users/{account}/bookmarks',
             'followers': f'/users/{account}/followers', 'following': f'/users/{account}/following',
             'profile': '/users/me'}
    params = {'max_results': 100}
    if category in ('posts', 'likes', 'bookmarks'):
        params.update({'post.fields': POST_FIELDS, 'expansions': 'author_id,attachments.media_keys,referenced_posts',
                       'media.fields': MEDIA_FIELDS, 'user.fields': USER_FIELDS})
    elif category == 'dms':
        params.update({'dm_event.fields': 'id,text,created_at,dm_conversation_id,event_type,attachments,entities',
                       'expansions': 'sender_id,participant_ids,attachments.media_keys',
                       'media.fields': MEDIA_FIELDS, 'user.fields': USER_FIELDS})
    elif category == 'profile':
        params = {'user.fields': USER_FIELDS}
    else:
        params['user.fields'] = USER_FIELDS
    if cursor.get('next_token'):
        params['pagination_token'] = cursor['next_token']
    return paths[category], params


def map_category(account, category, raw, includes, download):
    from x import map_post
    if category in ('likes', 'bookmarks', 'dms'):
        item, files = map_post(raw, includes, download)
        if category == 'dms':
            item.update(kind='message' if raw.get('event_type') == 'MessageCreate' else 'dm_event',
                        conversation=raw.get('dm_conversation_id'), sender=raw.get('sender_id'))
            item['data'] = {'event': raw, 'includes': includes,
                            'connector_warnings': item['data']['connector_warnings']}
        else:
            kind = 'like' if category == 'likes' else 'bookmark'
            item.update(id=f'{kind}:{raw["id"]}', kind=kind, sender=account, timestamp=None,
                        conversation=None)
            item['data']['relation_owner'] = account
            item['data']['relation_time_available'] = False
        return item, files
    kind = {'followers': 'follower', 'following': 'following', 'profile': 'profile'}[category]
    return dict(id=f'{kind}:{raw["id"]}', kind=kind, conversation=None, sender=str(raw['id']),
                timestamp=None, text=raw.get('name') or raw.get('username'), attachments=[],
                data={'user': raw, 'relation_owner': account, 'includes': includes}), []


def warning(destination, account, category, message):
    destination.send(batch('twitter', account, warnings=[f'X {category}: {message}; this category is incomplete and will retry.']))


def import_category_page(client, destination, vault, state, account, category):
    granted = getattr(client, 'auth', {}).get('scope')
    required = CATEGORY_SCOPES[category]
    if granted is not None and required not in granted.split():
        warning(destination, account, category, f'authorization did not grant {required}; authorize again to include it')
        return False
    if category == 'posts':
        from x import import_page
        try:
            return import_page(client, destination, vault, state, account)
        except XSourceError as exc:
            warning(destination, account, category, str(exc))
            return False
    cursor = state.get('categories', {}).get(category, {})
    path, params = category_request(account, category, cursor)
    try:
        response = client.request(path, params)
    except XSourceError as exc:
        warning(destination, account, category, str(exc))
        return False
    rows = response.get('data', [])
    if isinstance(rows, dict):
        rows = [rows]
    includes = response.get('includes', {})
    errors = response.get('errors', [])
    if errors:
        # Preserve the response's original API error metadata inside this owner's archive.
        digest = hashlib.sha256(json.dumps(errors, sort_keys=True).encode()).hexdigest()
        destination.send(batch('twitter', account, objects=[dict(id=f'api-error:{category}:{digest}',
            kind='api_error', conversation=None, sender=None, timestamp=None, text=None,
            attachments=[], data={'category': category, 'errors': errors})],
            warnings=[f'X {category}: API returned partial errors; available objects are retained, cursor is unchanged.']))
    try:
        for raw in rows:
            downloader = getattr(client, 'download_media', None)
            if downloader is None:
                from x import fetch_media
                downloader = fetch_media
            item, files = map_category(account, category, raw, includes, downloader)
            destination.send(batch('twitter', account, [item], files, item['data'].get('connector_warnings', [])))
    except XSourceError as exc:
        warning(destination, account, category, str(exc))
        return False
    if errors:
        return False
    next_state = copy.deepcopy(state)
    next_cursor = next_state.setdefault('categories', {}).setdefault(category, {})
    next_token = response.get('meta', {}).get('next_token')
    if next_token:
        next_cursor['next_token'] = next_token
    else:
        next_cursor.pop('next_token', None)
        # An empty successful category still has a durable destination receipt before its checkpoint.
        destination.send(batch('twitter', account, warnings=[
            f'X {category}: completed currently available API snapshot; source retention and permissions still limit coverage.']))
    vault.save(destination.checkpoint_name, next_state)
    state.clear()
    state.update(next_state)
    return bool(next_token)


def import_cycle(client, destination, vault, state, account, categories=None):
    for category in categories or selected_categories():
        for _ in range(10000):
            if not import_category_page(client, destination, vault, state, account, category):
                break
        else:
            warning(destination, account, category, 'per-cycle 10000-page bound reached; checkpoint saved for the next cycle')
