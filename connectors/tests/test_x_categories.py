import copy
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import Vault, ConnectorError
from x_categories import import_category_page, import_cycle, requested_scopes, selected_categories
from unittest.mock import patch
from io import BytesIO
from email.message import Message


class Destination:
    checkpoint_name = 'cursor'
    def __init__(self):
        self.batches = []
    def send(self, batch):
        self.batches.append(copy.deepcopy(batch))


class CategoryTests(unittest.TestCase):
    def test_every_category_uses_read_only_endpoint_and_archive_native_ids(self):
        cases = [
            ('likes', '/users/42/liked_tweets', {'id': '99', 'text': 'liked', 'author_id': '8'}, 'like', 'like:99'),
            ('bookmarks', '/users/42/bookmarks', {'id': '99', 'text': 'saved', 'author_id': '8'}, 'bookmark', 'bookmark:99'),
            ('followers', '/users/42/followers', {'id': '8', 'name': 'foreign'}, 'follower', 'follower:8'),
            ('following', '/users/42/following', {'id': '8', 'name': 'foreign'}, 'following', 'following:8'),
            ('dms', '/dm_events', {'id': '99', 'text': 'private', 'sender_id': '8',
              'dm_conversation_id': '8-42', 'event_type': 'MessageCreate',
              'created_at': '2026-10-27T08:00:00Z'}, 'message', '99'),
        ]
        with tempfile.TemporaryDirectory() as directory:
            for category, path, raw, kind, native_id in cases:
                with self.subTest(category=category):
                    class Client:
                        auth = {}
                        def request(inner, endpoint, params):
                            self.assertEqual(endpoint, path)
                            self.assertEqual(params['max_results'], 100)
                            if category == 'dms':
                                self.assertIn('dm_event.fields', params)
                            if category in ('likes', 'bookmarks'):
                                self.assertIn('post.fields', params)
                            return {'data': [raw], 'meta': {}}
                    destination = Destination()
                    state = {}
                    vault = Vault(Path(directory), b'x' * 32)
                    self.assertFalse(import_category_page(Client(), destination, vault, state, '42', category))
                    item = destination.batches[0]['objects'][0]
                    self.assertEqual(item['kind'], kind)
                    self.assertEqual(item['id'], native_id)
                    self.assertEqual(destination.batches[0]['source'], 'twitter')
                    if category == 'dms':
                        self.assertEqual(item['sender'], '8')
                        self.assertEqual(item['conversation'], '8-42')
                        self.assertEqual(item['timestamp'], raw['created_at'])
                    if category in ('likes', 'bookmarks'):
                        self.assertIsNone(item['timestamp'])  # Target post date is not the time the user liked it.

    def test_category_pagination_is_saved_only_after_destination_ack(self):
        class Client:
            auth = {}
            def request(self, path, params):
                return {'data': [{'id': '8', 'name': 'foreign'}], 'meta': {'next_token': 'page-two'}}
        with tempfile.TemporaryDirectory() as directory:
            vault = Vault(Path(directory), b'x' * 32)
            state = {}
            destination = Destination()
            self.assertTrue(import_category_page(Client(), destination, vault, state, '42', 'followers'))
            saved = vault.load('cursor')
            self.assertEqual(saved['categories']['followers']['next_token'], 'page-two')
            def rejected(batch):
                raise ConnectorError('revoked')
            destination.send = rejected
            with self.assertRaises(ConnectorError):
                import_category_page(Client(), destination, vault, state, '42', 'followers')
            self.assertEqual(vault.load('cursor'), saved)

    def test_denied_category_is_recorded_as_warning_and_does_not_stop_other_categories(self):
        from x_categories import XSourceError
        seen = []
        class Client:
            auth = {}
            def request(self, path, params):
                seen.append(path)
                if path == '/dm_events':
                    raise XSourceError(403)
                return {'data': [{'id': '8', 'name': 'foreign'}], 'meta': {}}
        with tempfile.TemporaryDirectory() as directory:
            destination = Destination()
            state = {}
            import_cycle(Client(), destination, Vault(Path(directory), b'x' * 32), state,
                         '42', ['dms', 'followers'])
            self.assertEqual(seen, ['/dm_events', '/users/42/followers'])
            self.assertIn('403', destination.batches[0]['warnings'][0])
            self.assertEqual(destination.batches[1]['objects'][0]['kind'], 'follower')
            self.assertNotIn('dms', state.get('categories', {}))

    def test_partial_posts_are_retained_without_advancing_affected_cursor(self):
        class Client:
            auth = {}
            def request(self, path, params):
                if path.endswith('/tweets'):
                    return {'data': [{'id': '99', 'text': 'available'}],
                            'errors': [{'status': 403, 'resource_id': 'inaccessible-reference'}]}
                return {'data': [{'id': '8', 'name': 'foreign'}]}
        with tempfile.TemporaryDirectory() as directory:
            destination = Destination()
            state = {'since_id': '50'}
            import_cycle(Client(), destination, Vault(Path(directory), b'x' * 32), state,
                         '42', ['posts', 'followers'])
            objects = [obj for batch in destination.batches for obj in batch['objects']]
            self.assertTrue(any(obj['id'] == '99' and obj['kind'] == 'post' for obj in objects))
            self.assertTrue(any(obj['kind'] == 'api_error' for obj in objects))
            self.assertEqual(state['since_id'], '50')
            self.assertTrue(any(obj['kind'] == 'follower' for obj in objects))

    def test_granted_scopes_skip_unavailable_endpoints_with_explicit_warning(self):
        class Client:
            auth = {'scope': 'tweet.read users.read offline.access'}
            def request(self, *args):
                self.fail('A missing granted scope must prevent the request')
        with tempfile.TemporaryDirectory() as directory:
            destination = Destination()
            import_category_page(Client(), destination, Vault(Path(directory), b'x' * 32), {}, '42', 'likes')
            self.assertIn('like.read', destination.batches[0]['warnings'][0])

    def test_dm_download_auth_is_scoped_to_official_private_endpoint(self):
        from x import fetch_media
        from x_categories import XSourceError
        seen = []
        class Response(BytesIO):
            def __init__(self):
                super().__init__(b'original-bytes')
                self.headers = Message()
                self.headers['Content-Type'] = 'image/jpeg'
        def opened(request, **kwargs):
            seen.append(request)
            return Response()
        with patch('x.OPENER.open', opened):
            self.assertEqual(fetch_media('https://api.x.com/2/dm_conversations/media/dm/media/resource', token='private-user-token')[0], b'original-bytes')
            self.assertEqual(seen[-1].headers['Authorization'], 'Bearer private-user-token')
            fetch_media('https://pbs.twimg.com/media/image.jpg', token='private-user-token')
            self.assertNotIn('Authorization', seen[-1].headers)
            with self.assertRaises(XSourceError):
                fetch_media('https://unapproved.invalid/media.jpg', token='private-user-token')
            self.assertEqual(len(seen), 2)

    def test_default_categories_maximize_supported_reads_and_explicit_subset_limits_scopes(self):
        self.assertEqual(set(selected_categories('')), {'posts', 'dms', 'likes', 'bookmarks', 'followers', 'following', 'profile'})
        self.assertEqual(selected_categories('posts,dms'), ['posts', 'dms'])
        self.assertEqual(set(requested_scopes(['posts', 'dms']).split()), {'tweet.read', 'users.read', 'offline.access', 'dm.read'})
        self.assertNotIn('write', requested_scopes())
        with self.assertRaises(ConnectorError):
            selected_categories('unsupported')


if __name__ == '__main__':
    unittest.main()
