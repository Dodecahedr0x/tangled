"""Owner-scoped browser pairing and durable supervision of the existing connectors.

Only the loopback control server may call this module. Provider credentials, native
sessions, PKCE verifiers, and ingest capabilities remain in encrypted host storage.
"""
import asyncio
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import signal
import shutil
import stat
import sys
import time
from urllib.parse import urlencode, urlsplit

from common import ConnectorError, Vault, private_file, validate_destination

CONNECTORS = Path(__file__).resolve().parents[1]
PUBLIC_FIELDS = ('id', 'source', 'status', 'expires_at', 'generation', 'account', 'qr_svg',
                 'authorize_url', 'connection_id', 'error')
TERMINAL = ('connected', 'expired')


class WhatsAppVault(Vault):
    """The existing Node connector's WCV1 authenticated-encryption wire format."""
    def load(self, name, default=None):
        path = self.path(name)
        if not path.exists():
            return {} if default is None else default
        blob = private_file(path)
        try:
            if blob[:4] != b'WCV1':
                raise ValueError('version')
            return json.loads(self.cipher.decrypt(blob[4:16], blob[32:] + blob[16:32], name.encode()))
        except Exception:
            raise ConnectorError('Cannot decrypt WhatsApp state') from None

    def save(self, name, value):
        # Reuse Vault's atomic fsync/rename, then convert before that atomic write.
        import tempfile
        nonce = os.urandom(12)
        ciphertext = self.cipher.encrypt(nonce, json.dumps(value, separators=(',', ':')).encode(), name.encode())
        blob = b'WCV1' + nonce + ciphertext[-16:] + ciphertext[:-16]
        fd, temporary = tempfile.mkstemp(prefix='.state-', dir=self.directory)
        try:
            with os.fdopen(fd, 'wb') as stream:
                os.fchmod(stream.fileno(), 0o600)
                stream.write(blob)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, self.path(name))
            directory_fd = os.open(self.directory, os.O_RDONLY)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)


class SourceAdapter:
    def available(self):
        return True, None

    async def start(self, record, vault):
        raise ConnectorError('Source unavailable')

    async def step(self, record, vault, values):
        raise ConnectorError('This source does not accept this challenge')


class TelegramAdapter(SourceAdapter):
    def available(self):
        if not importlib.util.find_spec('telethon'):
            return False, 'Install the connector Python requirements on the pairing host.'
        try:
            int(os.environ['TELEGRAM_API_ID'])
            private_file(os.environ['TELEGRAM_API_HASH_FILE'])
        except (KeyError, ValueError, OSError, ConnectorError):
            return False, 'The pairing host needs Telegram API application credentials.'
        return True, None

    async def start(self, record, vault):
        record['status'] = 'phone'

    async def step(self, record, vault, values):
        from telethon import TelegramClient
        from telethon.errors import SessionPasswordNeededError
        from telethon.sessions import StringSession
        auth = vault.load('auth')
        client = TelegramClient(StringSession(auth.get('session', '')),
            int(os.environ['TELEGRAM_API_ID']),
            private_file(os.environ['TELEGRAM_API_HASH_FILE']).decode().strip(),
            flood_sleep_threshold=0, request_retries=1, connection_retries=2,
            receive_updates=False)
        try:
            await client.connect()
            if record['status'] == 'phone':
                phone = values.get('phone', '')
                if not re.fullmatch(r'\+[1-9][0-9]{5,15}', phone):
                    raise ConnectorError('Enter a phone number in international format.')
                record['phone'] = phone
                try:
                    sent = await client.send_code_request(phone)
                    record.update(phone_code_hash=sent.phone_code_hash, status='code')
                except SessionPasswordNeededError:
                    record['status'] = 'password'
            elif record['status'] == 'code':
                if not values.get('code'):
                    raise ConnectorError('Enter the Telegram login code.')
                try:
                    await client.sign_in(phone=record['phone'], code=values['code'],
                        phone_code_hash=record['phone_code_hash'])
                except SessionPasswordNeededError:
                    record['status'] = 'password'
            elif record['status'] == 'password':
                if not values.get('password'):
                    raise ConnectorError('Enter the Telegram two-step password.')
                await client.sign_in(password=values['password'])
            else:
                raise ConnectorError('This Telegram challenge is no longer active.')
            if await client.is_user_authorized():
                me = await client.get_me()
                if not me or me.bot:
                    raise ConnectorError('A personal Telegram account is required.')
                record.update(status='authorized', account=str(me.id))
                record.pop('phone', None)
                record.pop('phone_code_hash', None)
        finally:
            vault.save('auth', {'session': client.session.save()})
            await client.disconnect()


class XAdapter(SourceAdapter):
    def available(self):
        redirect = os.environ.get('X_HOSTED_REDIRECT_URI', '')
        parsed = urlsplit(redirect)
        if (not os.environ.get('X_CLIENT_ID') or not parsed.hostname or parsed.query or
            parsed.fragment or parsed.username or parsed.password or
            parsed.path != '/api/imports/pairing/callback/x' or
            (parsed.scheme != 'https' and not (parsed.scheme == 'http' and
                parsed.hostname in ('localhost', '127.0.0.1', '::1')))):
            return False, 'The pairing host needs X OAuth client configuration and its registered HTTPS callback.'
        try:
            if os.environ.get('X_CLIENT_SECRET_FILE'):
                private_file(os.environ['X_CLIENT_SECRET_FILE'])
        except (OSError, ConnectorError):
            return False, 'The X application secret file is unavailable or not private.'
        return True, None

    async def start(self, record, vault):
        from x_categories import requested_scopes
        verifier = secrets.token_urlsafe(48)
        challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b'=').decode()
        record.update(status='redirect', verifier=verifier, oauth_state=secrets.token_urlsafe(32),
            redirect_uri=os.environ['X_HOSTED_REDIRECT_URI'])
        record['authorize_url'] = 'https://x.com/i/oauth2/authorize?' + urlencode(dict(
            response_type='code', client_id=os.environ['X_CLIENT_ID'],
            redirect_uri=record['redirect_uri'], scope=requested_scopes(),
            state=record['oauth_state'], code_challenge=challenge, code_challenge_method='S256'))

    async def callback(self, record, vault, code):
        from x import XClient
        client = XClient(vault)
        await asyncio.to_thread(client.token_request, dict(grant_type='authorization_code', code=code,
            redirect_uri=record['redirect_uri'], code_verifier=record.pop('verifier')))
        profile = await asyncio.to_thread(client.request, '/users/me')
        account = str(profile.get('data', {}).get('id', ''))
        if not account:
            raise ConnectorError('X did not return the authenticated personal account.')
        record.update(status='authorized', account=account)
        record.pop('authorize_url', None)


class WhatsAppAdapter(SourceAdapter):
    def available(self):
        if (not shutil.which(os.environ.get('TANGLED_NODE', 'node')) or not
            (CONNECTORS / 'whatsapp/node_modules/@whiskeysockets/baileys').is_dir()):
            return False, 'Install Node 22 or newer and the WhatsApp connector dependencies on the pairing host.'
        return True, None

    async def start(self, record, vault):
        record['status'] = 'qr'
        # Broker owns the actual child so cancellation and erasure await its exit.


class Broker:
    def __init__(self, vault, adapters=None):
        self.vault = vault
        # server.main acquires the exclusive root Vault lock before construction.
        self.cleanup_root_temporaries()
        self.records = vault.load('pairings')
        self.adapters = adapters if adapters is not None else {
            'telegram': TelegramAdapter(), 'twitter': XAdapter(), 'whatsapp': WhatsAppAdapter()}
        self.processes = {}
        self.tasks = {}
        self.closed = False
        # Derive per-session keys without storing another plaintext key file.
        self.secret = private_file(os.environ['TANGLED_SECRET_FILE']) if os.environ.get('TANGLED_SECRET_FILE') else None

    def cleanup_root_temporaries(self):
        """Remove only uncommitted root-index files, never canonical/owner data.

        Root index saves and this function are synchronous on the same broker
        event loop; no await permits an index save to interleave. The service's
        exclusive Vault lock excludes another broker process. Native worker and
        provider threads write separate per-session Vault directories.
        """
        try:
            for path in self.vault.directory.iterdir():
                if not path.name.startswith('.state-'):
                    continue
                try:
                    kind = path.lstat().st_mode
                    if not (stat.S_ISREG(kind) or stat.S_ISLNK(kind)):
                        continue
                    path.unlink()  # Unlink a symlink itself, never its target.
                except FileNotFoundError:
                    continue
            # Always sync, including a retry after unlink succeeded but the
            # previous directory sync failed. Empty enumeration is not a durable
            # erasure acknowledgment by itself.
            directory_fd = os.open(self.vault.directory, os.O_RDONLY)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
        except OSError:
            raise ConnectorError('Cannot remove retained pairing temporary files; credential erasure is pending.') from None

    def persist(self):
        self.vault.save('pairings', self.records)

    def forget_record(self, identity):
        # Root index writes are synchronous on this event loop. Retain the retry
        # identity if any pre-rename or post-rename durability step fails; another
        # attempt can safely commit the same removal after credentials are gone.
        record = self.records.pop(identity)
        try:
            self.persist()
        except Exception:
            self.records[identity] = record
            raise

    def source_vault(self, record):
        cls = WhatsAppVault if record['source'] == 'whatsapp' else Vault
        directory = self.vault.directory / record['id']
        if directory.is_symlink():
            raise ConnectorError('Invalid session directory')
        result = cls.__new__(cls)
        result.directory = directory
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        if directory.stat().st_mode & 0o077:
            raise ConnectorError('Session directory must be private')
        result.cipher = self.vault.cipher
        return result

    def public(self, record):
        return {name: record[name] for name in PUBLIC_FIELDS if name in record}

    def availability(self):
        sources = []
        for source, adapter in self.adapters.items():
            available, reason = adapter.available()
            sources.append(dict(source=source, available=available, **({'reason': reason} if reason else {})))
        return dict(available=any(row['available'] for row in sources), sources=sources)

    def owned(self, owner, identity):
        record = self.records.get(identity)
        if not record or not secrets.compare_digest(record['owner'], owner):
            raise ConnectorError('Pairing not found for this account.')
        return record

    async def clear_challenge(self, record):
        await self.stop_child(record['id'])
        directory = self.vault.directory / record['id']
        if directory.exists():
            shutil.rmtree(directory)
        for key in ('qr_svg', 'authorize_url', 'verifier', 'oauth_state', 'phone', 'phone_code_hash'):
            record.pop(key, None)

    async def expire(self, record):
        if record['status'] == 'failed':
            await self.clear_challenge(record)
            self.persist()
        elif record['status'] not in TERMINAL and record['expires_at'] <= time.time():
            await self.clear_challenge(record)
            record.update(status='expired')
            self.persist()

    async def start(self, owner, source, generation=0):
        if not isinstance(generation, int) or isinstance(generation, bool) or generation < 0:
            raise ConnectorError('Invalid host generation.')
        if source == 'x':
            source = 'twitter'
        adapter = self.adapters.get(source)
        if not adapter or not adapter.available()[0]:
            raise ConnectorError('Hosted pairing is not configured for this source.')
        for record in list(self.records.values()):
            await self.expire(record)
        # Terminal attempts retain safe status until the next start, then are
        # pruned. Abandoned logins cannot permanently exhaust an owner's quota.
        for identity, record in list(self.records.items()):
            if record['status'] in ('expired', 'failed'):
                del self.records[identity]
        self.persist()
        # Bound active native challenges and connected sessions.
        if sum(1 for r in self.records.values() if r['owner'] == owner) >= 20 or len(self.records) >= 1000:
            raise ConnectorError('Pairing limit reached; disconnect an existing pairing first.')
        record = dict(id=secrets.token_hex(24), owner=owner, source=source,
            status='starting', expires_at=int(time.time()) + 600, attempts=0, generation=generation)
        self.records[record['id']] = record
        self.persist()
        try:
            await asyncio.wait_for(adapter.start(record, self.source_vault(record)), 120)
            if source == 'whatsapp':
                self.tasks[record['id']] = asyncio.create_task(self.whatsapp_login(record))
        except Exception:
            record.update(status='failed', error='The source could not start pairing. Check host setup and retry.')
            await self.clear_challenge(record)
        self.persist()
        return self.public(record)

    async def status(self, owner, identity):
        record = self.owned(owner, identity)
        await self.expire(record)
        return self.public(record)

    async def step(self, owner, identity, values):
        record = self.owned(owner, identity)
        await self.expire(record)
        if record['status'] not in ('phone', 'code', 'password') or record['attempts'] >= 8:
            raise ConnectorError('This pairing challenge is no longer active. Start again.')
        if not isinstance(values, dict) or any(k not in ('phone', 'code', 'password') or
            not isinstance(v, str) or len(v) > 1024 for k, v in values.items()):
            raise ConnectorError('Invalid pairing input.')
        record['attempts'] += 1
        try:
            await asyncio.wait_for(self.adapters[record['source']].step(record, self.source_vault(record), values), 120)
            record.pop('error', None)
        except Exception:
            record['error'] = 'The source rejected this challenge or is unavailable. Check the input and retry.'
        if record['attempts'] >= 8 and record['status'] != 'authorized':
            record['expires_at'] = 0
            await self.expire(record)
        self.persist()
        return self.public(record)

    async def callback(self, owner, oauth_state, code):
        record = next((r for r in self.records.values() if r['owner'] == owner and
            r.get('oauth_state') and secrets.compare_digest(r['oauth_state'], oauth_state)), None)
        if not record:
            raise ConnectorError('OAuth state is invalid for this signed-in account.')
        await self.expire(record)
        if record['status'] != 'redirect' or not code or len(code) > 4096:
            raise ConnectorError('OAuth challenge expired or was denied.')
        record.pop('oauth_state', None)  # One use, persisted before exchanging the code.
        self.persist()
        try:
            await asyncio.wait_for(self.adapters['twitter'].callback(record, self.source_vault(record), code), 120)
        except Exception:
            record.update(status='failed', error='X authorization failed or was denied. Start pairing again.')
            await self.clear_challenge(record)
        self.persist()
        return self.public(record)

    async def bind(self, owner, identity, binding):
        record = self.owned(owner, identity)
        await self.expire(record)
        if record['status'] == 'connected':
            if record.get('binding') != binding:
                raise ConnectorError('Pairing already belongs to a different host connection.')
            return self.public(record)
        if record['status'] != 'authorized' or not record.get('account'):
            raise ConnectorError('Native source authentication is required before connecting.')
        if (set(binding) != {'connection_id', 'token'} or not all(isinstance(v, str) and
            0 < len(v) <= 512 and not re.search(r'[\r\n]', v) for v in binding.values())):
            raise ConnectorError('Invalid host connection capability.')
        # Stop older account worker before issuing the new destination binding.
        await self.stop_scope(owner, record['source'], record['account'], exclude=identity)
        await self.stop_child(identity)
        self.source_vault(record).save('hosted-binding', binding)
        record.update(binding=binding, connection_id=binding['connection_id'], status='connected')
        record.pop('qr_svg', None)
        self.persist()
        self.launch_worker(record)
        return self.public(record)

    def environment(self, record):
        if not self.secret:
            raise ConnectorError('Hosted worker encryption key is not configured.')
        destination = validate_destination(os.environ['TANGLED_HOST_URL'])
        environment = dict(os.environ, TANGLED_STATE_DIR=str(self.source_vault(record).directory),
            TANGLED_HOSTED='1', TANGLED_SOURCE=record['source'], TANGLED_URL=destination,
            TANGLED_ACCOUNT=record.get('account', ''), PYTHONUNBUFFERED='1')
        if record.get('binding'):
            environment['TANGLED_CONNECTION_ID'] = record['binding']['connection_id']
        return environment

    def launch_worker(self, record):
        identity = record['id']
        if identity not in self.tasks or self.tasks[identity].done():
            self.tasks[identity] = asyncio.create_task(self.supervise(record))

    async def supervise(self, record):
        identity = record['id']
        while not self.closed and identity in self.records and record['status'] == 'connected':
            try:
                environment = self.environment(record)
                if record['source'] == 'whatsapp':
                    # Stale Node lock belongs to the child we have already awaited.
                    lock = self.source_vault(record).directory / 'runner.lock'
                    lock.unlink(missing_ok=True)
                command = [sys.executable, str(CONNECTORS / 'hosted/worker.py'), identity, record['source'], 'run']
                process = await asyncio.create_subprocess_exec(*command, env=environment,
                    stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.DEVNULL,
                    stderr=asyncio.subprocess.DEVNULL)
                self.processes[identity] = process
                record['pid'] = process.pid
                record.pop('error', None)
                self.persist()
                await process.wait()
                if identity in self.records:
                    record['error'] = 'The source worker stopped; the host will retry from its saved checkpoint.'
                    self.persist()
            except asyncio.CancelledError:
                raise
            except Exception:
                record['error'] = 'The host could not run this source worker; check operator configuration.'
                self.persist()
            await asyncio.sleep(30)

    async def whatsapp_login(self, record):
        identity = record['id']
        try:
            environment = self.environment(record)
            environment['TANGLED_HOSTED_LOGIN'] = '1'
            process = await asyncio.create_subprocess_exec(sys.executable,
                str(CONNECTORS / 'hosted/worker.py'), identity, record['source'], 'login', env=environment,
                stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.DEVNULL, limit=512 * 1024)
            self.processes[identity] = process
            record['pid'] = process.pid
            self.persist()
            while line := await process.stdout.readline():
                # Only fixed structured events emitted by the owned connector are parsed.
                try:
                    event = json.loads(line)
                except (ValueError, UnicodeError):
                    continue
                if isinstance(event.get('qr_svg'), str) and len(event['qr_svg']) < 256 * 1024:
                    record['qr_svg'] = event['qr_svg']
                if isinstance(event.get('account'), str) and 0 < len(event['account']) < 256:
                    record.update(account=event['account'], status='authorized')
                    record.pop('qr_svg', None)
                self.persist()
            await process.wait()
            if record['status'] != 'authorized':
                record.update(status='failed', error='WhatsApp pairing stopped. Start again to request a fresh QR.')
                self.persist()
        except asyncio.CancelledError:
            raise
        except Exception:
            record.update(status='failed', error='WhatsApp pairing could not start. Check the host runtime.')
            self.persist()

    async def surviving_process(self, record, pid):
        if not isinstance(pid, int) or pid <= 1:
            return False
        check = await asyncio.create_subprocess_exec('ps', '-p', str(pid), '-o', 'command=',
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL)
        output, _ = await check.communicate()
        if check.returncode == 1:
            return False
        if check.returncode != 0:
            raise ConnectorError('Cannot verify surviving source worker; retry erasure.')
        command = output.decode().strip()
        source_script = 'telegram.py' if record['source'] == 'telegram' else ('x.py' if record['source'] == 'twitter' else 'whatsapp/index.mjs')
        # ps does not shell-quote paths. Match complete arguments without losing
        # connector paths containing spaces or interpreting them as shell code.
        marker = re.search(r'(?<!\S)' + re.escape(record['id']) + r'(?!\S)', command)
        script = any(re.search(r'(?<!\S)' + re.escape(str(path)) + r'(?!\S)', command)
            for path in (CONNECTORS / source_script, CONNECTORS / 'hosted/worker.py'))
        return bool(marker and script)

    async def stop_orphan(self, record):
        directory = self.vault.directory / record['id']
        durable = self.source_vault(record).load('hosted-process') if directory.exists() else {}
        for pid in {record.get('pid'), durable.get('pid')} - {None}:
            if not await self.surviving_process(record, pid):
                continue
            for signum, retries in ((signal.SIGTERM, 100), (signal.SIGKILL, 20)):
                try:
                    os.kill(pid, signum)
                except ProcessLookupError:
                    break
                for _ in range(retries):
                    if not await self.surviving_process(record, pid):
                        break
                    await asyncio.sleep(0.1)
                if not await self.surviving_process(record, pid):
                    break
            else:
                raise ConnectorError('A source worker is still running; credential erasure is pending.')
        record.pop('pid', None)

    async def stop_child(self, identity):
        task = self.tasks.pop(identity, None)
        if task:
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
        process = self.processes.pop(identity, None)
        if process and process.returncode is None:
            process.terminate()
            try:
                await asyncio.wait_for(process.wait(), 10)
            except asyncio.TimeoutError:
                process.kill()
                await process.wait()
        if identity in self.records:
            await self.stop_orphan(self.records[identity])

    async def stop_scope(self, owner, source=None, account=None, exclude=None):
        for identity, record in list(self.records.items()):
            if (identity != exclude and record['owner'] == owner and
                (source is None or record['source'] == source) and
                (account is None or record.get('account') == account or not record.get('account'))):
                await self.stop_child(identity)
                directory = self.vault.directory / identity
                if directory.exists():
                    shutil.rmtree(directory)
                self.forget_record(identity)
        self.cleanup_root_temporaries()
        return {'stopped': True}

    async def disconnect(self, owner, identity):
        record = self.owned(owner, identity)
        await self.stop_child(identity)
        directory = self.vault.directory / identity
        if directory.exists():
            shutil.rmtree(directory)
        # Complete fragment unlink/fsync while the canonical owner/session
        # identity still exists, so a failed disconnect remains retryable by ID.
        self.cleanup_root_temporaries()
        self.forget_record(identity)
        return {'stopped': True, 'connection_id': record.get('connection_id')}

    async def resume(self):
        for record in list(self.records.values()):
            await self.stop_child(record['id'])
            await self.expire(record)
            if record['status'] == 'connected':
                self.launch_worker(record)
            elif record['source'] == 'whatsapp' and record['status'] == 'qr':
                self.tasks[record['id']] = asyncio.create_task(self.whatsapp_login(record))

    async def maintenance(self):
        while not self.closed:
            for record in list(self.records.values()):
                await self.expire(record)
            await asyncio.sleep(5)

    async def shutdown(self):
        self.closed = True
        for identity in set(self.tasks) | set(self.processes):
            await self.stop_child(identity)
