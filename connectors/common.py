"""Local encrypted state and the connection-scoped Tangled ingest protocol."""
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile
import time
from urllib import error, parse, request

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

MAX_MEDIA = int(os.environ.get('TANGLED_MAX_MEDIA_BYTES', 8 * 1024 * 1024))
MAX_BATCH = 128 * 1024 * 1024


class ConnectorError(Exception):
    """Safe operational error: never includes remote bodies or credentials."""


def private_file(path):
    path = Path(path).expanduser()
    if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077:
        raise ConnectorError('Secret file must be a regular non-symlink file with mode 0600')
    return path.read_bytes()


def state_dir(source):
    path = Path(os.environ.get('TANGLED_STATE_DIR', f'~/.local/share/tangled-connectors/{source}')).expanduser()
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink() or path.stat().st_mode & 0o077:
        raise ConnectorError('State directory must be private (mode 0700), and not a symlink')
    return path


class Vault:
    def __init__(self, directory, secret):
        if len(secret) < 32:
            raise ConnectorError('Encryption key file must contain at least 32 random bytes')
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.cipher = AESGCM(hashlib.sha256(secret).digest())

    @classmethod
    def from_env(cls, source):
        key_path = os.environ.get('TANGLED_SECRET_FILE')
        if not key_path:
            raise ConnectorError('Set TANGLED_SECRET_FILE to a private random encryption key file')
        return cls(state_dir(source), private_file(key_path))

    def path(self, name):
        if not re.fullmatch(r'[a-zA-Z0-9_-]+', name):
            raise ConnectorError('Invalid local state name')
        return self.directory / (name + '.enc')

    def load(self, name, default=None):
        path = self.path(name)
        if not path.exists():
            return {} if default is None else default
        try:
            blob = private_file(path)
            if blob[:4] != b'TCV1':
                raise ValueError('version')
            return json.loads(self.cipher.decrypt(blob[4:16], blob[16:], name.encode()))
        except Exception:
            raise ConnectorError('Cannot decrypt local state; verify key and state integrity') from None

    def save(self, name, value):
        nonce = os.urandom(12)
        blob = b'TCV1' + nonce + self.cipher.encrypt(nonce,
            json.dumps(value, separators=(',', ':')).encode(), name.encode())
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

    def lock(self):
        # Held for the process lifetime; OS releases it after a crash.
        stream = open(self.directory / 'runner.lock', 'a+b')
        os.chmod(stream.name, 0o600)
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            stream.close()
            raise ConnectorError('Another connector is using this state directory') from None
        return stream


class NoRedirect(request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OPENER = request.build_opener(NoRedirect)


def validate_destination(url):
    parsed = parse.urlsplit(url)
    if (parsed.username or parsed.password or parsed.query or parsed.fragment or
        not parsed.hostname or parsed.scheme not in ('http', 'https')):
        raise ConnectorError('Destination must be a base HTTP(S) URL without credentials or query')
    if parsed.scheme != 'https' and parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
        raise ConnectorError('Remote destinations require HTTPS')
    return url.rstrip('/')


class Destination:
    def __init__(self, url, connection, token_file, sleep=time.sleep):
        self.url = validate_destination(url) + '/api/imports/connections/' + parse.quote(connection, safe='') + '/ingest'
        self.token_file = token_file
        self.sleep = sleep
        self.token_provider = None

    @classmethod
    def from_env(cls):
        try:
            if os.environ.get('TANGLED_HOSTED') == '1':
                vault = Vault.from_env(os.environ['TANGLED_SOURCE'])
                binding = vault.load('hosted-binding')
                if binding.get('connection_id') != os.environ['TANGLED_CONNECTION_ID']:
                    raise ConnectorError('Hosted destination binding mismatch')
                destination = cls(os.environ['TANGLED_URL'], binding['connection_id'], None)
                destination.token_provider = lambda: vault.load('hosted-binding')['token']
                return destination
            return cls(os.environ['TANGLED_URL'], os.environ['TANGLED_CONNECTION_ID'], os.environ['TANGLED_INGEST_TOKEN_FILE'])
        except KeyError:
            raise ConnectorError('Set TANGLED_URL, TANGLED_CONNECTION_ID and TANGLED_INGEST_TOKEN_FILE') from None

    @property
    def checkpoint_name(self):
        return 'cursor-' + hashlib.sha256(self.url.encode()).hexdigest()[:32]

    def send(self, batch):
        body = json.dumps(batch, separators=(',', ':')).encode()
        if len(body) > MAX_BATCH:
            raise ConnectorError('Batch exceeds 128 MiB; lower TANGLED_MAX_MEDIA_BYTES')
        token = self.token_provider() if self.token_provider else private_file(self.token_file).decode().strip()
        if not token or '\n' in token or '\r' in token:
            raise ConnectorError('Ingest token is empty or malformed')
        for attempt in range(5):
            try:
                req = request.Request(self.url, data=body, headers={
                    'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'}, method='POST')
                with OPENER.open(req, timeout=60) as response:
                    if not 200 <= response.status < 300:
                        raise ConnectorError('Destination did not acknowledge durable ingestion')
                return
            except error.HTTPError as exc:
                status = exc.code
                exc.close()
                if status != 429 and status < 500:
                    raise ConnectorError(f'Destination rejected import (HTTP {status}); checkpoint unchanged') from None
            except (error.URLError, TimeoutError, OSError):
                pass
            if attempt < 4:
                self.sleep(min(2 ** attempt, 30))
        raise ConnectorError('Destination unavailable after 5 attempts; checkpoint unchanged')


def deliver(destination, batch, checkpoint):
    destination.send(batch)
    checkpoint()


def batch(source, account, objects=None, files=None, warnings=None):
    return dict(source=source, account=str(account), objects=objects or [], files=files or [], warnings=warnings or [])


def imported_file(name, media_type, data):
    return dict(name=name, media_type=media_type or 'application/octet-stream',
                data_base64=base64.b64encode(data).decode())


def check_account(account, cursor):
    expected = os.environ.get('TANGLED_ACCOUNT')
    if expected and expected != str(account):
        raise ConnectorError('Authenticated source account differs from TANGLED_ACCOUNT')
    if cursor.get('account') and cursor['account'] != str(account):
        raise ConnectorError('Source account changed; use a new connection and state directory')
    cursor['account'] = str(account)
