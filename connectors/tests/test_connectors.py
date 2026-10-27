import base64
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import Destination, ConnectorError, Vault, deliver
import x
from x import map_post, source_request
from unittest.mock import patch
from telegram import map_message


class DestinationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.token = Path(self.temp.name) / 'token'
        self.token.write_text('private-ingest-token\n')
        self.token.chmod(0o600)
        self.requests = []
        self.statuses = [200]
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                owner.requests.append((self.path, self.headers['Authorization'],
                    json.loads(self.rfile.read(int(self.headers['Content-Length'])))))
                self.send_response(owner.statuses.pop(0))
                self.end_headers()
                self.wfile.write(b'{}')

            def do_GET(self):
                owner.requests.append((self.path, self.headers.get('Authorization')))
                self.send_response(owner.statuses.pop(0))
                self.end_headers()
                self.wfile.write(b'{}')

            def log_message(self, *args):
                pass

        self.server = HTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        self.dest = Destination(f'http://127.0.0.1:{self.server.server_port}',
            'connection/id', self.token, sleep=lambda _: None)
        self.batch = dict(source='twitter', account='42', objects=[], files=[], warnings=[])

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.temp.cleanup()

    def test_authorization_exact_contract_and_transient_retry(self):
        self.statuses = [503, 429, 200]
        self.dest.send(self.batch)
        self.assertEqual(len(self.requests), 3)
        path, auth, batch = self.requests[-1]
        self.assertEqual(path, '/api/imports/connections/connection%2Fid/ingest')
        self.assertEqual(auth, 'Bearer private-ingest-token')
        self.assertEqual(batch, self.batch)

    def test_rejected_destination_does_not_checkpoint_or_retry(self):
        self.statuses = [401]
        checkpoints = []
        with self.assertRaises(ConnectorError):
            deliver(self.dest, self.batch, lambda: checkpoints.append('advanced'))
        self.assertEqual(checkpoints, [])
        self.assertEqual(len(self.requests), 1)

    def test_bounded_destination_failure(self):
        self.statuses = [503] * 5
        with self.assertRaises(ConnectorError):
            self.dest.send(self.batch)
        self.assertEqual(len(self.requests), 5)

    def test_advance_only_after_success(self):
        checkpoints = []
        deliver(self.dest, self.batch, lambda: checkpoints.append(len(self.requests)))
        self.assertEqual(checkpoints, [1])

    def test_bounded_x_source_failure_and_read_authorization(self):
        self.statuses = [503] * 5
        with self.assertRaises(ConnectorError):
            source_request(f'http://127.0.0.1:{self.server.server_port}/source',
                           token='source-user-token', sleep=lambda _: None)
        self.assertEqual(len(self.requests), 5)
        self.assertEqual(self.requests[0][1], 'Bearer source-user-token')

    def test_remote_cleartext_destination_rejected(self):
        with self.assertRaises(ConnectorError):
            Destination('http://example.com', 'id', self.token)


class VaultTests(unittest.TestCase):
    def test_encrypted_roundtrip_permissions_and_wrong_secret(self):
        with tempfile.TemporaryDirectory() as temp:
            vault = Vault(Path(temp), b'a' * 32)
            vault.save('auth', {'token': 'secret-session'})
            stored = (Path(temp) / 'auth.enc').read_bytes()
            self.assertNotIn(b'secret-session', stored)
            self.assertEqual((Path(temp) / 'auth.enc').stat().st_mode & 0o777, 0o600)
            self.assertEqual(vault.load('auth'), {'token': 'secret-session'})
            with self.assertRaises(ConnectorError):
                Vault(Path(temp), b'b' * 32).load('auth')


class OAuthTests(unittest.TestCase):
    def test_x_pkce_callback_rejects_wrong_state_and_encrypts_refresh_credentials(self):
        import hashlib
        import socket
        from urllib import parse, request, error
        with tempfile.TemporaryDirectory() as temp:
            vault = Vault(Path(temp), b'a' * 32)
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0))
                port = sock.getsockname()[1]
            captured = {}
            callback_errors = []
            workers = []
            def printed(value, **kwargs):
                url = value.split('\\n')[-1]
                values = parse.parse_qs(parse.urlsplit(url).query)
                captured.update(values)
                def callback():
                    try:
                        with self.assertRaises(error.HTTPError) as rejected:
                            request.urlopen(f'http://127.0.0.1:{port}/callback?state=wrong&code=bad')
                        self.assertEqual(rejected.exception.code, 400)
                        rejected.exception.close()
                        valid = parse.urlencode({'state': values['state'][0], 'code': 'valid-code'})
                        with request.urlopen(f'http://127.0.0.1:{port}/callback?{valid}') as response:
                            self.assertEqual(response.status, 200)
                    except Exception as exc:
                        callback_errors.append(exc)
                worker = threading.Thread(target=callback)
                worker.start()
                workers.append(worker)
            def token_request(url, **kwargs):
                form = kwargs['form']
                self.assertEqual(url, 'https://api.x.com/2/oauth2/token')
                self.assertEqual(form['code'], 'valid-code')
                expected = base64.urlsafe_b64encode(hashlib.sha256(form['code_verifier'].encode()).digest()).rstrip(b'=').decode()
                self.assertEqual(captured['code_challenge'], [expected])
                self.assertEqual(captured['code_challenge_method'], ['S256'])
                self.assertEqual(captured['scope'], ['tweet.read users.read offline.access dm.read like.read bookmark.read follows.read'])
                return {'access_token': 'access', 'refresh_token': 'refresh', 'expires_in': 7200}
            config = {'X_CLIENT_ID': 'client', 'X_REDIRECT_URI': f'http://127.0.0.1:{port}/callback'}
            with patch.dict(os.environ, config, clear=True), patch('builtins.print', printed), patch('x.source_request', token_request):
                x.XClient(vault).login()
            for worker in workers:
                worker.join(timeout=3)
            self.assertEqual(callback_errors, [])
            self.assertEqual(vault.load('auth')['refresh_token'], 'refresh')
            self.assertNotIn(b'refresh', (Path(temp) / 'auth.enc').read_bytes())


class CursorTests(unittest.TestCase):
    def test_x_paginated_backfill_keeps_lower_watermark_until_all_pages_acknowledged(self):
        with tempfile.TemporaryDirectory() as temp:
            vault = Vault(Path(temp), b'a' * 32)
            state = {'since_id': '50'}
            class Client:
                def request(self, path, params):
                    self.params = params
                    return {'data': [{'id': '100', 'text': 'newest'}], 'meta': {'next_token': 'older-page'}}
            class Dest:
                checkpoint_name = 'cursor'
                def send(self, batch):
                    pass
            client, destination = Client(), Dest()
            self.assertTrue(x.import_page(client, destination, vault, state, '42'))
            saved = vault.load('cursor')
            self.assertEqual(saved['since_id'], '50')
            self.assertEqual(saved['next_token'], 'older-page')
            self.assertEqual(saved['newest_id'], '100')
            def failure(path, params):
                self.assertEqual(params['pagination_token'], 'older-page')
                return {'data': [{'id': '90', 'text': 'older'}], 'meta': {}}
            client.request = failure
            def reject(batch):
                raise ConnectorError('destination unavailable')
            destination.send = reject
            with self.assertRaises(ConnectorError):
                x.import_page(client, destination, vault, state, '42')
            self.assertEqual(vault.load('cursor'), saved)
            destination.send = lambda batch: None
            self.assertFalse(x.import_page(client, destination, vault, state, '42'))
            self.assertEqual(vault.load('cursor')['since_id'], '100')
            self.assertNotIn('next_token', vault.load('cursor'))

    def test_telegram_download_limit_stops_before_checkpoint(self):
        import asyncio
        from telegram import bounded_media
        class Client:
            async def download_media(self, message, **kwargs):
                kwargs['progress_callback'](9 * 1024 * 1024, 10 * 1024 * 1024)
                return b'oversize'
        with self.assertRaises(ConnectorError):
            asyncio.run(bounded_media(Client(), object()))

    def test_telegram_bounded_source_failure(self):
        import asyncio
        from telegram import source_call
        attempts = []
        async def unavailable():
            attempts.append(1)
            raise ConnectionError()
        async def no_sleep(_):
            pass
        with patch('telegram.asyncio.sleep', no_sleep), self.assertRaises(ConnectorError):
            asyncio.run(source_call(unavailable))
        self.assertEqual(len(attempts), 5)


class MappingTests(unittest.TestCase):
    def test_x_foreign_sender_timestamp_native_id_media_and_unknown_fields(self):
        post = dict(id='987', text='hello', author_id='9', conversation_id='123',
                    created_at='2026-10-27T08:00:00Z', unknown={'preserve': True},
                    attachments={'media_keys': ['m1']})
        item, files = map_post(post, {'media': [{'media_key': 'm1', 'type': 'photo',
                              'url': 'https://pbs.twimg.com/image.jpg'}]},
                              lambda _: (b'original-image', 'image/jpeg'))
        self.assertEqual(item['id'], '987')
        self.assertEqual(item['sender'], '9')
        self.assertEqual(item['timestamp'], post['created_at'])
        self.assertEqual(item['data']['post']['unknown'], post['unknown'])
        self.assertEqual(base64.b64decode(files[0]['data_base64']), b'original-image')
        self.assertEqual(item['attachments'], [files[0]['name']])

    def test_x_missing_media_expansion_is_explicit(self):
        item, files = map_post({'id': '99', 'attachments': {'media_keys': ['missing']}}, {},
                               lambda _: self.fail('No source media URL exists'))
        self.assertEqual(files, [])
        self.assertTrue(any('missing' in warning for warning in item['data']['connector_warnings']))

    def test_telegram_contact_snapshot_preserves_foreign_profile_and_unknown_metadata(self):
        from telegram import map_contact
        raw = {'id': 44, 'first_name': 'Foreign', 'phone': 'private-phone', 'unknown': {'retain': True}}
        item = map_contact(raw, {'user_id': 44, 'mutual': True})
        self.assertEqual(item['kind'], 'contact')
        self.assertEqual(item['sender'], '44')
        self.assertEqual(item['data']['user'], raw)
        self.assertEqual(item['data']['contact']['mutual'], True)

    def test_telegram_attachment_uses_safe_generated_name(self):
        raw = {'id': 5, 'message': 'file', 'original_filename': '..'}
        item, files = map_message(-10023, raw, b'bytes', '..', 'application/octet-stream')
        self.assertEqual(files[0]['name'], 'telegram/-10023/5/media')
        self.assertEqual(item['data']['original_filename'], '..')

    def test_telegram_distinct_ids_and_raw_metadata(self):
        raw = {'id': 5, 'message': 'same', 'date': '2026-10-27T08:00:00+00:00',
               'from_id': {'user_id': 44}, 'unknown': [1, 2]}
        item, files = map_message(-10023, raw, b'bytes', 'photo.jpg', 'image/jpeg')
        other, _ = map_message(-10024, raw, None, None, None)
        self.assertEqual(item['id'], '-10023:5')
        self.assertNotEqual(item['id'], other['id'])
        self.assertEqual(item['sender'], '44')
        self.assertEqual(item['data']['unknown'], [1, 2])
        self.assertEqual(base64.b64decode(files[0]['data_base64']), b'bytes')


if __name__ == '__main__':
    unittest.main()
