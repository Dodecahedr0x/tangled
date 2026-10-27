"""Real encrypted broker persistence/control tests; source network boundaries are isolated."""
import asyncio
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from common import ConnectorError, Vault
from hosted.service import Broker, SourceAdapter


class ControlledSource(SourceAdapter):
    """Only native provider transport is replaced; broker and vault remain real."""
    async def start(self, record, vault):
        record['status'] = 'phone'

    async def step(self, record, vault, values):
        if values != {'phone': '+33123456789'}:
            raise ConnectorError('Invalid phone')
        vault.save('auth', {'session': 'native-session-secret'})
        record.update(status='authorized', account='12345')


class BrokerTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.directory = Path(self.tmp.name)
        self.vault = Vault(self.directory, b'k' * 32)
        self.broker = Broker(self.vault, adapters={'telegram': ControlledSource()})

    async def asyncTearDown(self):
        await self.broker.shutdown()
        self.tmp.cleanup()

    async def test_owner_isolation_and_no_secret_in_public_status(self):
        item = await self.broker.start('alice', 'telegram')
        with self.assertRaises(ConnectorError):
            await self.broker.status('mallory', item['id'])
        item = await self.broker.step('alice', item['id'], {'phone': '+33123456789'})
        self.assertEqual(item['account'], '12345')
        self.assertEqual(item['status'], 'authorized')
        self.assertNotIn('native-session-secret', json.dumps(item))
        with self.assertRaises(ConnectorError):
            await self.broker.bind('mallory', item['id'], {'connection_id': 'c', 'token': 't'})
        for path in self.directory.rglob('*.enc'):
            self.assertNotIn(b'native-session-secret', path.read_bytes())

    async def test_restart_retains_authorized_session_and_erasure_removes_its_bytes(self):
        item = await self.broker.start('alice', 'telegram')
        await self.broker.step('alice', item['id'], {'phone': '+33123456789'})
        other = await self.broker.start('bob', 'telegram')
        broker = Broker(self.vault, adapters={'telegram': ControlledSource()})
        self.assertEqual((await broker.status('alice', item['id']))['account'], '12345')
        await broker.stop_scope('alice', 'telegram', '12345')
        with self.assertRaises(ConnectorError):
            await broker.status('alice', item['id'])
        self.assertEqual((await broker.status('bob', other['id']))['status'], 'phone')
        self.assertFalse((self.directory / item['id']).exists())

    async def test_challenge_expiry_rejects_step_and_removes_source_credentials(self):
        item = await self.broker.start('alice', 'telegram')
        record = self.broker.records[item['id']]
        record['expires_at'] = 1
        self.broker.source_vault(record).save('auth', {'session': 'expired-secret'})
        self.assertEqual((await self.broker.status('alice', item['id']))['status'], 'expired')
        with self.assertRaises(ConnectorError):
            await self.broker.step('alice', item['id'], {'phone': '+33123456789'})
        self.assertFalse((self.directory / item['id']).exists())

    async def test_binding_persists_encrypted_worker_secret_and_supervises_only_once(self):
        item = await self.broker.start('alice', 'telegram')
        await self.broker.step('alice', item['id'], {'phone': '+33123456789'})
        # Keep real durable binding; replace actual child process boundary only.
        with patch.object(self.broker, 'launch_worker') as launch:
            binding = {'connection_id': 'connection-one', 'token': 'ingest-secret-value'}
            result = await self.broker.bind('alice', item['id'], binding)
            self.assertEqual(result['status'], 'connected')
            self.assertEqual(result['connection_id'], 'connection-one')
            await self.broker.bind('alice', item['id'], binding)
            launch.assert_called_once()
        restarted = Broker(self.vault, adapters={'telegram': ControlledSource()})
        self.assertEqual(restarted.records[item['id']]['binding']['token'], 'ingest-secret-value')
        self.assertNotIn('ingest-secret-value', json.dumps(result))
        for path in self.directory.rglob('*.enc'):
            self.assertNotIn(b'ingest-secret-value', path.read_bytes())

    async def test_scope_stop_cancels_unfinished_challenge_for_source_and_preserves_other_owner(self):
        item = await self.broker.start('alice', 'telegram')
        other = await self.broker.start('bob', 'telegram')
        await self.broker.stop_scope('alice', 'telegram', None)
        with self.assertRaises(ConnectorError):
            await self.broker.status('alice', item['id'])
        self.assertEqual((await self.broker.status('bob', other['id']))['status'], 'phone')


if __name__ == '__main__':
    unittest.main()

class GenerationTests(unittest.IsolatedAsyncioTestCase):
    async def test_host_generation_survives_native_auth_and_broker_restart(self):
        with tempfile.TemporaryDirectory() as directory:
            vault = Vault(Path(directory), b'k' * 32)
            broker = Broker(vault, {'telegram': ControlledSource()})
            item = await broker.start('alice', 'telegram', generation=9)
            item = await broker.step('alice', item['id'], {'phone': '+33123456789'})
            self.assertEqual(item['generation'], 9)
            restarted = Broker(vault, {'telegram': ControlledSource()})
            self.assertEqual((await restarted.status('alice', item['id']))['generation'], 9)

class OrphanWorkerTests(unittest.IsolatedAsyncioTestCase):
    async def test_erasure_stops_a_surviving_worker_after_broker_restart(self):
        import sys
        from hosted.service import CONNECTORS
        with tempfile.TemporaryDirectory() as directory:
            vault = Vault(Path(directory), b'k' * 32)
            broker = Broker(vault, {'telegram': ControlledSource()})
            item = await broker.start('alice', 'telegram')
            # Real process represents an orphaned runner; argv binds its random
            # nonsecret session identity to the exact owned connector path.
            child = await asyncio.create_subprocess_exec(sys.executable, '-c',
                'import time; time.sleep(300)', str(CONNECTORS / 'telegram.py'), 'run', item['id'])
            try:
                broker.records[item['id']]['pid'] = child.pid
                broker.persist()
                restarted = Broker(vault, {'telegram': ControlledSource()})
                await restarted.stop_scope('alice')
                try:
                    await asyncio.wait_for(child.wait(), 2)
                except asyncio.TimeoutError:
                    self.fail('Source worker survived acknowledged credential erasure')
                self.assertIsNotNone(child.returncode)
            finally:
                if child.returncode is None:
                    child.kill()
                    await child.wait()

class RetentionTests(unittest.IsolatedAsyncioTestCase):
    async def test_twenty_expired_attempts_do_not_permanently_exhaust_owner_pairing_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            broker = Broker(Vault(Path(directory), b'k' * 32), {'telegram': ControlledSource()})
            for _ in range(20):
                item = await broker.start('alice', 'telegram')
                broker.records[item['id']]['expires_at'] = 1
                await broker.status('alice', item['id'])
            item = await broker.start('alice', 'telegram')
            self.assertEqual(item['status'], 'phone')

    async def test_failed_native_start_drops_partially_written_source_authorization(self):
        class FailedSource(ControlledSource):
            async def start(self, record, vault):
                vault.save('auth', {'session': 'partly-authorized-secret'})
                raise ConnectorError('provider identity lookup failed')
        with tempfile.TemporaryDirectory() as directory:
            broker = Broker(Vault(Path(directory), b'k' * 32), {'telegram': FailedSource()})
            item = await broker.start('alice', 'telegram')
            self.assertEqual(item['status'], 'failed')
            self.assertFalse((Path(directory) / item['id']).exists())

class RootTemporaryCleanupTests(unittest.IsolatedAsyncioTestCase):
    async def test_restart_removes_uncommitted_global_index_fragments_without_following_links(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            vault = Vault(root, b'k' * 32)
            broker = Broker(vault, {'telegram': ControlledSource()})
            other = await broker.start('bob', 'telegram')
            await broker.step('bob', other['id'], {'phone': '+33123456789'})
            canonical = vault.path('pairings').read_bytes()
            fragment = root / '.state-crashed-index'
            fragment.write_bytes(canonical)
            fragment.chmod(0o600)
            external = root / 'operator-keep'
            external.write_bytes(b'keep unrelated bytes')
            link = root / '.state-crashed-link'
            link.symlink_to(external)
            unrelated = root / '.state-operator-directory'
            unrelated.mkdir()
            restarted = Broker(vault, {'telegram': ControlledSource()})
            self.assertFalse(fragment.exists())
            self.assertFalse(link.is_symlink())
            self.assertEqual(external.read_bytes(), b'keep unrelated bytes')
            self.assertTrue(unrelated.is_dir())
            self.assertEqual(vault.path('pairings').read_bytes(), canonical)
            self.assertEqual((await restarted.status('bob', other['id']))['account'], '12345')
            self.assertEqual(restarted.source_vault(restarted.records[other['id']]).load('auth'), {'session': 'native-session-secret'})

    async def test_scope_erase_removes_global_fragments_and_preserves_other_owner_authorization(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            vault = Vault(root, b'k' * 32)
            broker = Broker(vault, {'telegram': ControlledSource()})
            items = {}
            for owner in ('alice', 'bob'):
                item = await broker.start(owner, 'telegram')
                await broker.step(owner, item['id'], {'phone': '+33123456789'})
                items[owner] = item
            # This is a real encrypted global index copy of both owner records.
            # It must disappear even though Bob's canonical authorization remains.
            fragment = root / '.state-crashed-index'
            fragment.write_bytes(vault.path('pairings').read_bytes())
            fragment.chmod(0o600)
            self.assertEqual(await broker.stop_scope('alice', 'telegram', '12345'), {'stopped': True})
            self.assertFalse(fragment.exists())
            self.assertFalse((root / items['alice']['id']).exists())
            self.assertEqual((await broker.status('bob', items['bob']['id']))['account'], '12345')
            self.assertEqual(broker.source_vault(broker.records[items['bob']['id']]).load('auth'), {'session': 'native-session-secret'})
            restarted = Broker(vault, {'telegram': ControlledSource()})
            with self.assertRaises(ConnectorError):
                await restarted.status('alice', items['alice']['id'])
            self.assertEqual((await restarted.status('bob', items['bob']['id']))['account'], '12345')

    async def test_scope_erase_does_not_acknowledge_failed_fragment_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            broker = Broker(Vault(root, b'k' * 32), {'telegram': ControlledSource()})
            fragment = root / '.state-crashed-index'
            fragment.write_bytes(b'encrypted credential fragment')
            original_unlink = Path.unlink
            def deny_fragment(path, *args, **kwargs):
                if path == fragment:
                    raise PermissionError('controlled cleanup failure')
                return original_unlink(path, *args, **kwargs)
            with patch.object(Path, 'unlink', deny_fragment):
                with self.assertRaises(ConnectorError):
                    await broker.stop_scope('alice')
            self.assertTrue(fragment.exists())

    async def test_failed_directory_sync_remains_pending_on_retry_after_fragment_unlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            broker = Broker(Vault(root, b'k' * 32), {'telegram': ControlledSource()})
            fragment = root / '.state-crashed-index'
            fragment.write_bytes(b'encrypted credential fragment')
            with patch('hosted.service.os.fsync', side_effect=OSError('controlled durability failure')):
                with self.assertRaises(ConnectorError):
                    await broker.stop_scope('alice')
                self.assertFalse(fragment.exists())
                with self.assertRaises(ConnectorError):
                    await broker.stop_scope('alice')

class DisconnectRetryTests(unittest.IsolatedAsyncioTestCase):
    async def test_failed_fragment_unlink_keeps_disconnect_retry_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            vault = Vault(root, b'k' * 32)
            broker = Broker(vault, {'telegram': ControlledSource()})
            item = await broker.start('alice', 'telegram')
            await broker.step('alice', item['id'], {'phone': '+33123456789'})
            fragment = root / '.state-crashed-index'
            fragment.write_bytes(vault.path('pairings').read_bytes())
            original_unlink = Path.unlink
            def deny_fragment(path, *args, **kwargs):
                if path == fragment:
                    raise PermissionError('controlled fragment failure')
                return original_unlink(path, *args, **kwargs)
            with patch.object(Path, 'unlink', deny_fragment):
                with self.assertRaises(ConnectorError):
                    await broker.disconnect('alice', item['id'])
            self.assertIn(item['id'], broker.records)
            self.assertIn(item['id'], vault.load('pairings'))
            self.assertEqual(await broker.disconnect('alice', item['id']), {'stopped': True, 'connection_id': None})
            self.assertFalse(fragment.exists())
            self.assertFalse((root / item['id']).exists())
            self.assertNotIn(item['id'], vault.load('pairings'))

    async def test_failed_directory_sync_keeps_disconnect_retry_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            vault = Vault(root, b'k' * 32)
            broker = Broker(vault, {'telegram': ControlledSource()})
            item = await broker.start('alice', 'telegram')
            fragment = root / '.state-crashed-index'
            fragment.write_bytes(vault.path('pairings').read_bytes())
            # Isolate the root cleanup durability boundary rather than the later
            # unrelated encrypted index write, which has its own regression below.
            real_cleanup = broker.cleanup_root_temporaries
            def fail_sync():
                with patch('hosted.service.os.fsync', side_effect=OSError('controlled directory sync failure')):
                    real_cleanup()
            with patch.object(broker, 'cleanup_root_temporaries', fail_sync):
                with self.assertRaises(ConnectorError):
                    await broker.disconnect('alice', item['id'])
            self.assertIn(item['id'], broker.records)
            self.assertIn(item['id'], vault.load('pairings'))
            self.assertEqual(await broker.disconnect('alice', item['id']), {'stopped': True, 'connection_id': None})
            self.assertFalse((root / item['id']).exists())
            self.assertNotIn(item['id'], vault.load('pairings'))

    async def test_failed_index_commit_keeps_scope_and_disconnect_retryable_without_losing_bob(self):
        for operation in ('disconnect', 'stop_scope'):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                vault = Vault(root, b'k' * 32)
                broker = Broker(vault, {'telegram': ControlledSource()})
                items = {}
                for owner in ('alice', 'bob'):
                    item = await broker.start(owner, 'telegram')
                    await broker.step(owner, item['id'], {'phone': '+33123456789'})
                    items[owner] = item
                before = vault.path('pairings').read_bytes()
                async def remove_alice():
                    if operation == 'disconnect':
                        return await broker.disconnect('alice', items['alice']['id'])
                    return await broker.stop_scope('alice', 'telegram', '12345')
                # Actual pre-rename commit boundary fails, leaving canonical bytes
                # unchanged. Provider/session side effects and retry remain real.
                with patch('common.os.replace', side_effect=OSError('controlled pre-rename persistence failure')):
                    with self.assertRaises(OSError):
                        await remove_alice()
                self.assertEqual(vault.path('pairings').read_bytes(), before)
                self.assertIn(items['alice']['id'], broker.records)
                self.assertTrue((await remove_alice())['stopped'])
                retained = vault.load('pairings')
                self.assertNotIn(items['alice']['id'], retained)
                self.assertIn(items['bob']['id'], retained)
                self.assertFalse((root / items['alice']['id']).exists())
                self.assertEqual(broker.source_vault(broker.records[items['bob']['id']]).load('auth'), {'session': 'native-session-secret'})

    async def test_post_rename_sync_failure_keeps_both_removal_operations_retryable(self):
        import os
        import stat
        for operation in ('disconnect', 'stop_scope'):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                vault = Vault(root, b'k' * 32)
                broker = Broker(vault, {'telegram': ControlledSource()})
                items = {}
                for owner in ('alice', 'bob'):
                    item = await broker.start(owner, 'telegram')
                    await broker.step(owner, item['id'], {'phone': '+33123456789'})
                    items[owner] = item
                async def remove_alice():
                    if operation == 'disconnect':
                        return await broker.disconnect('alice', items['alice']['id'])
                    return await broker.stop_scope('alice', 'telegram', '12345')
                real_persist, real_fsync = broker.persist, os.fsync
                def fail_directory_sync(fd):
                    if stat.S_ISDIR(os.fstat(fd).st_mode):
                        raise OSError('controlled post-rename index durability failure')
                    return real_fsync(fd)
                def fail_commit():
                    with patch('common.os.fsync', side_effect=fail_directory_sync):
                        real_persist()
                with patch.object(broker, 'persist', side_effect=fail_commit):
                    with self.assertRaises(OSError):
                        await remove_alice()
                # Rename occurred, but its durable acknowledgment failed. Memory
                # retains the retry identity and retry converges to the same disk state.
                self.assertNotIn(items['alice']['id'], vault.load('pairings'))
                self.assertIn(items['alice']['id'], broker.records)
                self.assertTrue((await remove_alice())['stopped'])
                retained = vault.load('pairings')
                self.assertNotIn(items['alice']['id'], retained)
                self.assertIn(items['bob']['id'], retained)
                self.assertEqual(broker.source_vault(broker.records[items['bob']['id']]).load('auth'), {'session': 'native-session-secret'})
