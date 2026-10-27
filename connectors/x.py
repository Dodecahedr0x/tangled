"""X user OAuth2 PKCE, own-post backfill and read-only incremental polling."""
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
import secrets
import sys
import time
from urllib import error, parse, request

from common import (ConnectorError, Destination, Vault, MAX_MEDIA, OPENER, batch,
                    check_account, imported_file, private_file)

from x_categories import (XSourceError, requested_scopes, selected_categories, import_cycle,
                          POST_FIELDS, USER_FIELDS, MEDIA_FIELDS)

API = 'https://api.x.com/2'


def source_request(url, token=None, form=None, basic=None, sleep=time.sleep):
    headers = {'Accept': 'application/json'}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    if basic:
        headers['Authorization'] = 'Basic ' + base64.b64encode(basic.encode()).decode()
    body = None
    if form is not None:
        body = parse.urlencode(form).encode()
        headers['Content-Type'] = 'application/x-www-form-urlencoded'
    for attempt in range(5):
        try:
            with OPENER.open(request.Request(url, data=body, headers=headers), timeout=60) as response:
                data = response.read(8 * 1024 * 1024 + 1)
                if len(data) > 8 * 1024 * 1024:
                    raise ConnectorError('X response exceeded 8 MiB limit')
                return json.loads(data)
        except error.HTTPError as exc:
            status = exc.code
            exc.close()
            if status != 429 and status < 500:
                raise XSourceError(status) from None
        except (error.URLError, TimeoutError, OSError):
            pass
        if attempt < 4:
            sleep(min(2 ** attempt, 30))
    raise XSourceError()


def fetch_media(url, token=None):
    parsed = parse.urlsplit(url)
    private_dm = (parsed.hostname == 'api.x.com' and parsed.path.startswith('/2/dm_conversations/media/') and token)
    if (parsed.scheme != 'https' or parsed.username or parsed.password or
            not (parsed.hostname in ('pbs.twimg.com', 'video.twimg.com') or private_dm)):
        raise XSourceError(message='Media URL is outside supported source CDN/authorized DM endpoints')
    for attempt in range(5):
        try:
            headers = {'Authorization': 'Bearer ' + token} if private_dm else {}
            with OPENER.open(request.Request(url, headers=headers), timeout=60) as response:
                if int(response.headers.get('Content-Length', '0')) > MAX_MEDIA:
                    return None, response.headers.get_content_type()
                data = response.read(MAX_MEDIA + 1)
                return (data if len(data) <= MAX_MEDIA else None), response.headers.get_content_type()
        except error.HTTPError as exc:
            status = exc.code
            exc.close()
            if status != 429 and status < 500:
                raise XSourceError(status, 'X media unavailable (HTTP ' + str(status) + ')') from None
        except (error.URLError, TimeoutError, OSError):
            pass
        if attempt < 4:
            time.sleep(2 ** attempt)
    raise XSourceError(message='X media unavailable after 5 attempts; category cursor unchanged')


def map_post(post, includes, download=fetch_media):
    files = []
    wanted = post.get('attachments', {}).get('media_keys', [])
    warnings = []
    for media in includes.get('media', []):
        if media.get('media_key') not in wanted:
            continue
        url = media.get('url')
        if media.get('type') in ('video', 'animated_gif'):
            variants = [v for v in media.get('variants', []) if v.get('content_type') == 'video/mp4']
            url = max(variants, key=lambda v: v.get('bit_rate', 0), default={}).get('url')
        if url:
            data, mime = download(url)
            if data is not None:
                name = f'twitter/{post["id"]}/{media["media_key"]}'
                files.append(imported_file(name, mime, data))
            else:
                warnings.append('Media exceeds configured byte limit: ' + str(media['media_key']))
        else:
            warnings.append('Media bytes unavailable in API expansion: ' + str(media.get('media_key')))
    returned_keys = {media.get('media_key') for media in includes.get('media', [])}
    for missing in set(wanted) - returned_keys:
        warnings.append('Media expansion unavailable: ' + str(missing))
    item = dict(id=post['id'], kind='post', conversation=post.get('conversation_id'),
                sender=post.get('author_id'), timestamp=post.get('created_at'), text=(post.get('note_post') or post.get('note_tweet') or {}).get('text') or post.get('text'),
                attachments=[f['name'] for f in files],
                data={'post': post, 'includes': includes, 'connector_warnings': warnings})
    return item, files


class XClient:
    def __init__(self, vault):
        self.vault = vault
        self.auth = vault.load('auth')
        self.client_id = os.environ.get('X_CLIENT_ID', '')
        secret_file = os.environ.get('X_CLIENT_SECRET_FILE')
        self.client_secret = private_file(secret_file).decode().strip() if secret_file else None

    def token_request(self, form):
        form['client_id'] = self.client_id
        basic = f'{self.client_id}:{self.client_secret}' if self.client_secret else None
        result = source_request(API + '/oauth2/token', form=form, basic=basic)
        if 'access_token' not in result:
            raise ConnectorError('X token endpoint returned no access token')
        self.auth.update(result)
        self.auth['expires_at'] = time.time() + int(result.get('expires_in', 7200))
        self.vault.save('auth', self.auth)

    def request(self, path, params=None):
        if self.auth.get('expires_at', float('inf')) < time.time() + 60:
            if not self.auth.get('refresh_token') or not self.client_id:
                raise ConnectorError('X session expired; interactive authorization required')
            self.token_request({'grant_type': 'refresh_token', 'refresh_token': self.auth['refresh_token']})
        token = self.auth.get('access_token')
        if not token:
            raise ConnectorError('X authorization required: run x.py login first')
        url = API + path + ('?' + parse.urlencode(params) if params else '')
        return source_request(url, token=token)

    def download_media(self, url):
        if self.auth.get('expires_at', float('inf')) < time.time() + 60:
            self.request('/users/me')
        return fetch_media(url, token=self.auth.get('access_token'))

    def login(self):
        token_file = os.environ.get('X_ACCESS_TOKEN_FILE')
        if token_file:
            # Must be a user OAuth2 token; app-only tokens fail /users/me.
            self.auth = {'access_token': private_file(token_file).decode().strip()}
            refresh_file = os.environ.get('X_REFRESH_TOKEN_FILE')
            if refresh_file:
                self.auth['refresh_token'] = private_file(refresh_file).decode().strip()
                self.auth['expires_at'] = 0  # Rotate immediately to establish known expiry.
            self.vault.save('auth', self.auth)
            return
        if not self.client_id:
            raise ConnectorError('Set X_CLIENT_ID for OAuth2 PKCE, or X_ACCESS_TOKEN_FILE for a user token')
        redirect = os.environ.get('X_REDIRECT_URI', 'http://127.0.0.1:8765/callback')
        parsed = parse.urlsplit(redirect)
        if (parsed.scheme != 'http' or parsed.hostname not in ('127.0.0.1', 'localhost')
                or not parsed.port or parsed.query or parsed.fragment or parsed.username):
            raise ConnectorError('X_REDIRECT_URI must be an HTTP loopback callback with explicit port')
        verifier = secrets.token_urlsafe(48)
        challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b'=').decode()
        state = secrets.token_urlsafe(32)
        result = {}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                callback = parse.urlsplit(self.path)
                values = parse.parse_qs(callback.query)
                valid = (callback.path == (parsed.path or '/') and
                         secrets.compare_digest(values.get('state', [''])[0], state))
                if valid and values.get('code'):
                    result['code'] = values['code'][0]
                elif valid and values.get('error'):
                    result['denied'] = True
                self.send_response(200 if result else 400)
                self.end_headers()
                self.wfile.write(b'Authorization received. Return to the connector terminal.' if result else b'Invalid callback.')

            def log_message(self, *args):
                pass

        with HTTPServer(('127.0.0.1', parsed.port), Handler) as server:
            server.timeout = 1
            authorize = 'https://x.com/i/oauth2/authorize?' + parse.urlencode(dict(
                response_type='code', client_id=self.client_id, redirect_uri=redirect,
                scope=requested_scopes(), state=state, code_challenge=challenge, code_challenge_method='S256'))
            print('Open this URL in your browser on this computer:\n' + authorize, flush=True)
            deadline = time.monotonic() + 300
            while not result and time.monotonic() < deadline:
                server.handle_request()
        if not result.get('code'):
            raise ConnectorError('X authorization denied or timed out after 5 minutes')
        self.token_request(dict(grant_type='authorization_code', code=result['code'],
                                redirect_uri=redirect, code_verifier=verifier))


def import_page(client, destination, vault, state, account):
    params = {'max_results': 100, 'post.fields': POST_FIELDS,
              'expansions': 'author_id,attachments.media_keys,referenced_posts,edit_history_post_ids',
              'media.fields': MEDIA_FIELDS, 'user.fields': USER_FIELDS}
    if state.get('since_id'):
        params['since_id'] = state['since_id']
    if state.get('next_token'):
        params['pagination_token'] = state['next_token']
    response = client.request(f'/users/{account}/tweets', params)
    api_errors = response.get('errors', [])
    posts = response.get('data', [])
    newest = max([int(p['id']) for p in posts] + [int(state.get('newest_id', 0))])
    for post in posts:
        item, files = map_post(post, response.get('includes', {}), getattr(client, 'download_media', fetch_media))
        destination.send(batch('twitter', account, [item], files, item['data']['connector_warnings']))
    if api_errors:
        digest = hashlib.sha256(json.dumps(api_errors, sort_keys=True).encode()).hexdigest()
        destination.send(batch('twitter', account, objects=[dict(id='api-error:posts:' + digest,
            kind='api_error', conversation=None, sender=None, timestamp=None, text=None,
            attachments=[], data={'category': 'posts', 'errors': api_errors})], warnings=[
                'X posts: partial API errors; available posts retained, cursor unchanged for retry.']))
        return False
    # Persist pagination after the whole page succeeds. Advance since_id only after all older pages succeed.
    next_state = dict(state, newest_id=str(newest))
    next_token = response.get('meta', {}).get('next_token')
    if next_token:
        next_state['next_token'] = next_token
    else:
        if newest:
            next_state['since_id'] = str(newest)
        next_state.pop('next_token', None)
        destination.send(batch('twitter', account, warnings=[
            'X posts: completed available own-post backfill; timeline retention, deletion and API access limits apply.']))
    vault.save(destination.checkpoint_name, next_state)
    state.clear()
    state.update(next_state)
    return bool(next_token)


def run(mode='run'):
    vault = Vault.from_env('twitter')
    with vault.lock():
        client = XClient(vault)
        if mode == 'login':
            client.login()
        account = str(client.request('/users/me')['data']['id'])
        print(f'X source account: {account}', flush=True)
        if mode == 'login':
            return
        destination = Destination.from_env()
        state = vault.load(destination.checkpoint_name)
        check_account(account, state)
        interval = max(60, int(os.environ.get('TANGLED_POLL_SECONDS', '120')))
        while True:
            import_cycle(client, destination, vault, state, account, selected_categories())
            if mode == 'once':
                return
            time.sleep(interval)


if __name__ == '__main__':
    try:
        mode = sys.argv[1] if len(sys.argv) > 1 else 'run'
        if mode not in ('login', 'run', 'once'):
            raise ConnectorError('Usage: x.py [login|run|once]')
        run(mode)
    except KeyboardInterrupt:
        pass
    except (ConnectorError, KeyError, ValueError) as exc:
        print(str(exc) if isinstance(exc, ConnectorError) else 'Invalid or missing connector configuration', file=sys.stderr)
        sys.exit(1)
    except Exception:
        print('X connector stopped after a source failure; restart to resume acknowledged pages', file=sys.stderr)
        sys.exit(1)
