import base64
import hashlib
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

from common import ConnectorError, Vault
from hosted.service import Broker, XAdapter, WhatsAppVault


class NativeAdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_x_pkce_owner_state_and_single_use_callback(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {
            'X_CLIENT_ID': 'operator-client',
            'X_HOSTED_REDIRECT_URI': 'https://host.example/api/imports/pairing/callback/x'}, clear=False):
            broker = Broker(Vault(Path(directory), b'k' * 32), {'twitter': XAdapter()})
            item = await broker.start('alice', 'twitter')
            query = parse_qs(urlsplit(item['authorize_url']).query)
            record = broker.records[item['id']]
            expected = base64.urlsafe_b64encode(hashlib.sha256(record['verifier'].encode()).digest()).rstrip(b'=').decode()
            self.assertEqual(query['code_challenge'], [expected])
            self.assertEqual(query['code_challenge_method'], ['S256'])
            self.assertEqual(query['redirect_uri'], ['https://host.example/api/imports/pairing/callback/x'])
            with self.assertRaises(ConnectorError):
                await broker.callback('mallory', query['state'][0], 'provider-code')
            verifier = record['verifier']
            def provider(url, token=None, form=None, basic=None, **unused):
                if url.endswith('/oauth2/token'):
                    self.assertEqual(form['code'], 'provider-code')
                    self.assertEqual(form['code_verifier'], verifier)
                    return {'access_token': 'provider-access-secret', 'refresh_token': 'refresh-secret', 'expires_in': 7200}
                self.assertTrue(url.endswith('/users/me'))
                self.assertEqual(token, 'provider-access-secret')
                return {'data': {'id': '778899'}}
            # XClient and its encrypted token persistence execute; only HTTPS transport is isolated.
            with patch('x.source_request', side_effect=provider):
                item = await broker.callback('alice', query['state'][0], 'provider-code')
            self.assertEqual((item['status'], item['account']), ('authorized', '778899'))
            with self.assertRaises(ConnectorError):
                await broker.callback('alice', query['state'][0], 'provider-code')
            self.assertEqual(broker.source_vault(record).load('auth')['refresh_token'], 'refresh-secret')

    async def test_whatsapp_vault_round_trip_is_authenticated_and_not_plaintext(self):
        with tempfile.TemporaryDirectory() as directory:
            vault = WhatsAppVault(Path(directory), b'k' * 32)
            vault.save('auth', {'creds': {'secret': 'wa-session-secret'}, 'keys': {}})
            self.assertEqual(vault.load('auth')['creds']['secret'], 'wa-session-secret')
            blob = vault.path('auth').read_bytes()
            self.assertEqual(blob[:4], b'WCV1')
            self.assertNotIn(b'wa-session-secret', blob)
            vault.path('auth').write_bytes(blob[:-1] + bytes([blob[-1] ^ 1]))
            with self.assertRaises(ConnectorError):
                vault.load('auth')

class TelegramNativeTests(unittest.IsolatedAsyncioTestCase):
    async def test_telegram_native_phone_code_then_two_step_password(self):
        from types import SimpleNamespace
        from telethon.errors import SessionPasswordNeededError
        from hosted.service import TelegramAdapter
        with tempfile.TemporaryDirectory() as directory:
            secret_file = Path(directory) / 'telegram-api-secret'
            secret_file.write_text('operator-api-hash')
            secret_file.chmod(0o600)
            class ProviderClient:
                def __init__(self, session, api_id, api_hash, **options):
                    self.session = session
                    self.authorized = False
                    assert (api_id, api_hash) == (123, 'operator-api-hash')
                async def connect(self): pass
                async def disconnect(self): pass
                async def send_code_request(self, phone):
                    assert phone == '+33123456789'
                    return SimpleNamespace(phone_code_hash='native-code-hash')
                async def sign_in(self, **values):
                    if 'code' in values:
                        assert values == {'phone': '+33123456789', 'code': '73920', 'phone_code_hash': 'native-code-hash'}
                        raise SessionPasswordNeededError(None)
                    assert values == {'password': 'user-two-step-secret'}
                    self.authorized = True
                async def is_user_authorized(self): return self.authorized
                async def get_me(self): return SimpleNamespace(id=112233, bot=False)
            with patch.dict(os.environ, {'TELEGRAM_API_ID': '123', 'TELEGRAM_API_HASH_FILE': str(secret_file)}), patch('telethon.TelegramClient', ProviderClient):
                broker = Broker(Vault(Path(directory), b'k' * 32), {'telegram': TelegramAdapter()})
                item = await broker.start('alice', 'telegram')
                item = await broker.step('alice', item['id'], {'phone': '+33123456789'})
                self.assertEqual(item['status'], 'code')
                item = await broker.step('alice', item['id'], {'code': '73920'})
                self.assertEqual(item['status'], 'password')
                item = await broker.step('alice', item['id'], {'password': 'user-two-step-secret'})
                self.assertEqual((item['status'], item['account']), ('authorized', '112233'))
                self.assertNotIn('user-two-step-secret', repr(broker.records))
                self.assertNotIn('native-code-hash', repr(item))
                for path in Path(directory).rglob('*.enc'):
                    self.assertNotIn(b'+33123456789', path.read_bytes())

class XFailureCleanupTests(unittest.IsolatedAsyncioTestCase):
    async def test_successful_token_exchange_failed_identity_lookup_removes_tokens(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {
            'X_CLIENT_ID': 'operator-client', 'X_CLIENT_SECRET_FILE': '',
            'X_HOSTED_REDIRECT_URI': 'https://host.example/api/imports/pairing/callback/x'}):
            broker = Broker(Vault(Path(directory), b'k' * 32), {'twitter': XAdapter()})
            item = await broker.start('alice', 'twitter')
            state = parse_qs(urlsplit(item['authorize_url']).query)['state'][0]
            def provider(url, **values):
                if url.endswith('/oauth2/token'):
                    return {'access_token': 'partially-authorized-token', 'refresh_token': 'partial-refresh'}
                raise ConnectorError('identity unavailable')
            with patch('x.source_request', side_effect=provider):
                item = await broker.callback('alice', state, 'valid-provider-code')
            self.assertEqual(item['status'], 'failed')
            self.assertFalse((Path(directory) / item['id']).exists())
            self.assertNotIn('partial-refresh', repr(broker.records))
            with self.assertRaises(ConnectorError):
                await broker.callback('alice', state, 'valid-provider-code')

class WhatsAppInteropTests(unittest.TestCase):
    def test_python_broker_binding_decrypts_in_existing_real_node_vault(self):
        import json
        import subprocess
        from hosted.service import CONNECTORS
        with tempfile.TemporaryDirectory() as directory:
            vault = WhatsAppVault(Path(directory), b'k' * 32)
            vault.save('hosted-binding', {'connection_id': 'native-binding', 'token': 'private-test-ingest'})
            script = '''
                import { Vault } from './connectors/whatsapp/core.mjs';
                const vault = new Vault(process.argv[1], Buffer.alloc(32, 107));
                const value = vault.load('hosted-binding');
                vault.save('node-written', { native: 'node-state' });
                process.stdout.write(JSON.stringify(value));
            '''
            result = subprocess.run(['node', '--input-type=module', '-e', script, directory],
                cwd=CONNECTORS.parent, check=True, capture_output=True, text=True, timeout=10)
            self.assertEqual(json.loads(result.stdout), {'connection_id': 'native-binding', 'token': 'private-test-ingest'})
            self.assertEqual(vault.load('node-written'), {'native': 'node-state'})
