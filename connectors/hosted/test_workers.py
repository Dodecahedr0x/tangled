import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from common import Destination, Vault


class WorkerCapabilityTests(unittest.TestCase):
    def test_existing_worker_reads_ingest_token_only_from_encrypted_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            key = Path(directory) / 'key'
            key.write_bytes(b'k' * 32)
            key.chmod(0o600)
            state = Path(directory) / 'session'
            state.mkdir(mode=0o700)
            vault = Vault(state, b'k' * 32)
            vault.save('hosted-binding', {'connection_id': 'c1', 'token': 'hosted-ingest-secret'})
            env = {'TANGLED_SECRET_FILE': str(key), 'TANGLED_STATE_DIR': str(state),
                'TANGLED_HOSTED': '1', 'TANGLED_SOURCE': 'telegram',
                'TANGLED_URL': 'http://127.0.0.1:4000', 'TANGLED_CONNECTION_ID': 'c1'}
            with patch.dict(os.environ, env, clear=True):
                destination = Destination.from_env()
                with patch('common.OPENER.open') as open_request:
                    open_request.return_value.__enter__.return_value.status = 200
                    destination.send({'source': 'telegram', 'account': '123', 'objects': []})
                    req = open_request.call_args.args[0]
                    self.assertEqual(req.get_header('Authorization'), 'Bearer hosted-ingest-secret')
                    self.assertTrue(req.full_url.endswith('/connections/c1/ingest'))
            self.assertNotIn(b'hosted-ingest-secret', vault.path('hosted-binding').read_bytes())
