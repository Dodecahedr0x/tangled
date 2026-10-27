"""Container configuration and real child lifecycle boundaries; no source calls."""
import asyncio
import os
from pathlib import Path
import signal
import stat
import sys
import tempfile
import unittest

from hosted.supervise import StartupError, prepare_environment, supervise


class ConfigurationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir='/private/tmp')
        self.addCleanup(self.tmp.cleanup)
        self.data = Path(self.tmp.name) / 'data'

    def test_container_creates_private_persisted_keys_and_matching_loopback_control(self):
        provider, broker = prepare_environment({'DATA_DIR': str(self.data), 'PORT': '10000',
            'PUBLIC_URL': 'https://tangled.example', 'X_CATEGORIES': 'posts,bookmarks'})
        self.assertIsNotNone(broker)
        self.assertEqual(provider['TANGLED_PAIRING_URL'], 'http://127.0.0.1:8792')
        self.assertEqual(broker['TANGLED_HOST_URL'], 'https://tangled.example')
        key = Path(broker['TANGLED_SECRET_FILE'])
        token = Path(provider['TANGLED_PAIRING_TOKEN_FILE'])
        self.assertEqual(len(key.read_bytes()), 32)
        self.assertEqual(len(token.read_text()), 64)
        self.assertNotEqual(key.read_bytes(), token.read_bytes())
        for path in [key, token]:
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(key.parent.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE(Path(broker['TANGLED_PAIRING_STATE_DIR']).stat().st_mode), 0o700)
        before = key.read_bytes(), token.read_bytes()
        _, restarted = prepare_environment({'DATA_DIR': str(self.data), 'PORT': '10000'})
        self.assertEqual(before, (Path(restarted['TANGLED_SECRET_FILE']).read_bytes(),
            Path(restarted['TANGLED_PAIRING_TOKEN_FILE']).read_bytes()))
        self.assertNotIn('TANGLED_SECRET_FILE', provider)
        self.assertNotIn('X_CATEGORIES', provider)
        self.assertEqual(broker['X_CATEGORIES'], 'posts,bookmarks')

    def test_operator_files_are_not_replaced_or_silently_repaired(self):
        key, token = Path(self.tmp.name) / 'operator-key', Path(self.tmp.name) / 'operator-token'
        key.write_bytes(b'operator-key-material-32-bytes-long')
        token.write_text('b' * 64)
        key.chmod(0o600)
        token.chmod(0o600)
        env = {'DATA_DIR': str(self.data), 'TANGLED_SECRET_FILE': str(key),
            'TANGLED_PAIRING_TOKEN_FILE': str(token)}
        _, broker = prepare_environment(env)
        self.assertEqual(broker['TANGLED_SECRET_FILE'], str(key))
        self.assertEqual(key.read_bytes(), b'operator-key-material-32-bytes-long')
        key.chmod(0o644)
        with self.assertRaises(StartupError):
            prepare_environment(env)
        self.assertEqual(key.stat().st_mode & 0o777, 0o644)
        key.chmod(0o600)
        alias = Path(self.tmp.name) / 'alias'
        alias.symlink_to(key)
        with self.assertRaises(StartupError):
            prepare_environment(dict(env, TANGLED_SECRET_FILE=str(alias)))
        with self.assertRaises(StartupError):
            prepare_environment(dict(env, TANGLED_SECRET_FILE=str(key.parent / 'missing')))
        self.assertFalse((key.parent / 'missing').exists())

    def test_disabling_pairing_removes_control_configuration_and_creates_no_secret(self):
        provider, broker = prepare_environment({'DATA_DIR': str(self.data),
            'TANGLED_HOSTED_PAIRING': '0', 'TANGLED_PAIRING_URL': 'http://127.0.0.1:8792',
            'TANGLED_PAIRING_TOKEN_FILE': '/unused'})
        self.assertIsNone(broker)
        self.assertNotIn('TANGLED_PAIRING_URL', provider)
        self.assertNotIn('TANGLED_PAIRING_TOKEN_FILE', provider)
        self.assertFalse(self.data.exists())

    def test_disabled_restart_preserves_control_for_retained_native_credentials(self):
        from common import Vault
        provider, broker = prepare_environment({'DATA_DIR': str(self.data)})
        key = Path(broker['TANGLED_SECRET_FILE']).read_bytes()
        state = Path(broker['TANGLED_PAIRING_STATE_DIR'])
        vault = Vault(state, key)
        vault.save('pairings', {'session': {'owner': 'alice', 'source': 'telegram', 'status': 'connected'}})
        native = Vault(state / 'session', key)
        native.save('auth', {'session': 'retained-native-session-secret'})
        disabled, child = prepare_environment({'DATA_DIR': str(self.data), 'TANGLED_HOSTED_PAIRING': '0'})
        self.assertIsNone(child)
        self.assertEqual(disabled['TANGLED_PAIRING_URL'], provider['TANGLED_PAIRING_URL'])
        self.assertEqual(disabled['TANGLED_PAIRING_TOKEN_FILE'], provider['TANGLED_PAIRING_TOKEN_FILE'])
        self.assertEqual(native.load('auth'), {'session': 'retained-native-session-secret'})
        Path(provider['TANGLED_PAIRING_TOKEN_FILE']).unlink()
        with self.assertRaises(StartupError):
            prepare_environment({'DATA_DIR': str(self.data), 'TANGLED_HOSTED_PAIRING': '0'})
        self.assertFalse(Path(provider['TANGLED_PAIRING_TOKEN_FILE']).exists())

    def test_disabled_restart_after_cleanup_can_remove_control_configuration(self):
        from common import Vault
        _, broker = prepare_environment({'DATA_DIR': str(self.data)})
        Vault(Path(broker['TANGLED_PAIRING_STATE_DIR']), Path(broker['TANGLED_SECRET_FILE']).read_bytes()).save('pairings', {})
        provider, child = prepare_environment({'DATA_DIR': str(self.data), 'TANGLED_HOSTED_PAIRING': '0'})
        self.assertIsNone(child)
        self.assertNotIn('TANGLED_PAIRING_URL', provider)

    def test_invalid_port_callback_destination_and_unsafe_persisted_directory_fail_closed(self):
        for extra in [{'TANGLED_PAIRING_PORT': '10000'}, {'TANGLED_PAIRING_PORT': '0'},
            {'TANGLED_PAIRING_URL': 'http://remote.example:8792'},
            {'TANGLED_HOST_URL': 'http://remote.example'}, {'TANGLED_HOSTED_PAIRING': 'tru'}]:
            with self.assertRaises(StartupError):
                prepare_environment(dict({'DATA_DIR': str(self.data), 'PORT': '10000'}, **extra))
        control = self.data / 'pairing-control'
        control.mkdir(parents=True, mode=0o755)
        control.chmod(0o755)
        with self.assertRaises(StartupError):
            prepare_environment({'DATA_DIR': str(self.data)})


class SupervisionTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir='/private/tmp')
        self.path = Path(self.tmp.name)

    async def asyncTearDown(self):
        self.tmp.cleanup()

    def child(self, name, fail=None):
        # The real child records only its own PID and shutdown marker.
        script = '''import os,signal,sys,time
from pathlib import Path
root=Path(sys.argv[1]); name=sys.argv[2]
(root/(name+'.pid')).write_text(str(os.getpid()))
def stop(*unused):
    with (root/'order').open('a') as stream: stream.write(name+'\\n')
    sys.exit(0)
signal.signal(signal.SIGTERM,stop)
if len(sys.argv)>3:
    time.sleep(0.2); sys.exit(int(sys.argv[3]))
while True: time.sleep(1)
'''
        command = [sys.executable, '-c', script, str(self.path), name]
        if fail is not None:
            command.append(str(fail))
        return command

    def environment(self):
        return dict(os.environ, DATA_DIR=str(self.path / 'data'), TANGLED_HOSTED_PAIRING='1')

    async def wait_started(self):
        for _ in range(500):
            if all((self.path / (name + '.pid')).exists() for name in ('provider', 'broker')):
                return
            await asyncio.sleep(0.01)
        self.fail('Owned children did not start')

    def assert_stopped(self, name):
        pid = int((self.path / (name + '.pid')).read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    async def test_broker_failure_stops_provider_and_propagates_failure(self):
        result = await supervise(self.child('provider'), self.child('broker', fail=7),
            self.environment(), shutdown_timeout=2)
        self.assertEqual(result, 7)
        self.assert_stopped('provider')
        self.assert_stopped('broker')
        self.assertEqual((self.path / 'order').read_text(), 'provider\n')

    async def test_provider_failure_stops_broker_before_returning(self):
        result = await supervise(self.child('provider', fail=9), self.child('broker'),
            self.environment(), shutdown_timeout=2)
        self.assertEqual(result, 9)
        self.assert_stopped('provider')
        self.assert_stopped('broker')
        self.assertEqual((self.path / 'order').read_text(), 'broker\n')

    async def test_sigterm_stops_broker_then_provider_and_reaps_both(self):
        task = asyncio.create_task(supervise(self.child('provider'), self.child('broker'),
            self.environment(), shutdown_timeout=2))
        try:
            await self.wait_started()
            os.kill(os.getpid(), signal.SIGTERM)
            self.assertEqual(await asyncio.wait_for(task, 5), 0)
            self.assertEqual((self.path / 'order').read_text(), 'broker\nprovider\n')
            self.assert_stopped('provider')
            self.assert_stopped('broker')
        finally:
            if not task.done():
                task.cancel()
                try:
                    await task
                except asyncio.CancelledError:
                    pass


if __name__ == '__main__':
    unittest.main()
