"""Run provider and its native pairing broker in one container under tini.

This entry point owns the two children. It never prints environment values or
credentials, never replaces persisted keys, and exits on either child's failure
so the deployment supervisor can restart the whole coherent service.
"""
import asyncio
import os
from pathlib import Path
import secrets
import signal
import stat
import sys
from urllib.parse import urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


class StartupError(Exception):
    """Safe startup diagnostic; never contains secret file contents."""


def _directory(path, private=False):
    path = Path(path).expanduser().absolute()
    if path.is_symlink():
        raise StartupError('Pairing directories must not be symlinks.')
    try:
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
        info = path.lstat()
    except OSError:
        raise StartupError('Cannot create or inspect the persisted pairing directory.') from None
    if not stat.S_ISDIR(info.st_mode) or (private and info.st_mode & 0o077):
        raise StartupError('Pairing control/state directories must be private directories (0700).')
    return path


def _secret(path, generated=None):
    path = Path(path).expanduser().absolute()
    if generated is not None:
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
        except FileExistsError:
            pass  # Existing deployment keys are never replaced or chmodded.
        except OSError:
            raise StartupError('Cannot create the persisted pairing secret file.') from None
        else:
            with os.fdopen(fd, 'wb') as stream:
                os.fchmod(stream.fileno(), 0o600)
                stream.write(generated)
                stream.flush()
                os.fsync(stream.fileno())
            directory_fd = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
    try:
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077 or info.st_size > 4096:
            raise StartupError('Pairing key/token files must be private regular non-symlink files (0600).')
        value = path.read_bytes()
    except OSError:
        raise StartupError('A configured pairing key/token file is missing or unreadable; it was not replaced.') from None
    return path, value


def _port(value, label):
    try:
        port = int(value)
    except (ValueError, TypeError):
        raise StartupError(label + ' must be a numeric port.') from None
    if not 1 <= port <= 65535:
        raise StartupError(label + ' must be between 1 and 65535.')
    return port


def _destination(value):
    try:
        url = urlsplit(value)
        valid = (url.hostname and url.scheme in ('https', 'http') and not url.username and
            not url.password and not url.query and not url.fragment and
            (url.scheme == 'https' or url.hostname in ('localhost', '127.0.0.1', '::1')))
        _ = url.port
    except ValueError:
        valid = False
    if not valid:
        raise StartupError('TANGLED_HOST_URL must be an HTTPS base URL (or HTTP loopback for local use), without credentials or query.')
    return value.rstrip('/')


def _retained_control(source):
    """Keep erasure fail-closed when opt-out leaves prior encrypted sessions."""
    data = Path(source.get('DATA_DIR', './data-provider')).expanduser().absolute()
    control = data / 'pairing-control'
    state = Path(source.get('TANGLED_PAIRING_STATE_DIR', str(control / 'state'))).expanduser().absolute()
    try:
        info = state.lstat()
    except FileNotFoundError:
        return None
    except OSError:
        raise StartupError('Cannot inspect prior pairing state; re-enable pairing before disconnecting or erasing source credentials.') from None
    if not stat.S_ISDIR(info.st_mode) or info.st_mode & 0o077:
        raise StartupError('Prior pairing state must remain a private non-symlink directory; re-enable pairing after repairing its configuration.')
    try:
        entries = {entry.name for entry in state.iterdir()} - {'runner.lock'}
    except OSError:
        raise StartupError('Prior pairing state cannot be read; source credential erasure must remain pending.') from None
    if not entries:
        return None
    key_path, key = _secret(source.get('TANGLED_SECRET_FILE', str(control / 'key')))
    token_path, token = _secret(source.get('TANGLED_PAIRING_TOKEN_FILE', str(control / 'control-token')))
    bearer = token.strip()
    if len(key) < 32 or len(bearer) < 32 or not bearer.isascii() or b'\r' in bearer or b'\n' in bearer or key_path == token_path or key == token:
        raise StartupError('Prior pairing key/token configuration is unsafe; re-enable pairing after restoring the original private files.')
    try:
        from common import Vault
        records = Vault(state, key).load('pairings')
    except Exception:
        raise StartupError('Prior pairing state cannot be decrypted; restore its original key and re-enable pairing to disconnect or erase source credentials.') from None
    if not records and entries <= {'pairings.enc'}:
        return None
    port = _port(source.get('TANGLED_PAIRING_PORT', '8792'), 'TANGLED_PAIRING_PORT')
    url = 'http://127.0.0.1:' + str(port)
    if source.get('TANGLED_PAIRING_URL', url) != url:
        raise StartupError('Prior pairing control URL must match its loopback port; re-enable pairing to finish credential erasure.')
    return url, str(token_path)


def prepare_environment(environment):
    """Validate operator config and create only absent default private secrets.

    Returns independent provider/broker environments. Explicit key/token files
    must exist already; a missing explicit file is treated as a configuration
    error rather than silently minting a different key.
    """
    source = dict(environment)
    provider = dict(source)
    # Source application configuration and encryption keys belong to the broker.
    source_only = {'TANGLED_SECRET_FILE', 'TANGLED_PAIRING_STATE_DIR', 'TANGLED_HOST_URL',
        'X_CLIENT_ID', 'X_CLIENT_SECRET_FILE', 'X_ACCESS_TOKEN_FILE', 'X_REFRESH_TOKEN_FILE',
        'X_HOSTED_REDIRECT_URI', 'X_CATEGORIES', 'TANGLED_NODE'}
    for name in tuple(provider):
        if name in source_only or name.startswith('TELEGRAM_'):
            del provider[name]
    enabled = source.get('TANGLED_HOSTED_PAIRING', '1')
    if enabled not in ('0', '1'):
        raise StartupError('TANGLED_HOSTED_PAIRING must be 1 (enabled) or 0 (disabled).')
    if enabled == '0':
        provider.pop('TANGLED_PAIRING_URL', None)
        provider.pop('TANGLED_PAIRING_TOKEN_FILE', None)
        retained = _retained_control(source)
        if retained is not None:
            provider.update(TANGLED_PAIRING_URL=retained[0], TANGLED_PAIRING_TOKEN_FILE=retained[1])
        return provider, None
    port = _port(source.get('PORT', '8787'), 'PORT')
    control_port = _port(source.get('TANGLED_PAIRING_PORT', '8792'), 'TANGLED_PAIRING_PORT')
    if port == control_port:
        raise StartupError('The provider and pairing control must use different ports.')
    control_url = 'http://127.0.0.1:' + str(control_port)
    if source.get('TANGLED_PAIRING_URL', control_url) != control_url:
        raise StartupError('TANGLED_PAIRING_URL must match the container loopback pairing port.')
    host_url = _destination(source.get('TANGLED_HOST_URL') or source.get('PUBLIC_URL') or
        source.get('RENDER_EXTERNAL_URL') or 'http://127.0.0.1:' + str(port))
    data = _directory(source.get('DATA_DIR', './data-provider'))
    control = _directory(data / 'pairing-control', private=True)
    key_name = source.get('TANGLED_SECRET_FILE')
    token_name = source.get('TANGLED_PAIRING_TOKEN_FILE')
    if ('TANGLED_SECRET_FILE' in source and not key_name) or ('TANGLED_PAIRING_TOKEN_FILE' in source and not token_name):
        raise StartupError('Configured pairing key/token file paths must not be empty.')
    key_path, key = _secret(key_name or control / 'key', None if key_name else secrets.token_bytes(32))
    token_path, token = _secret(token_name or control / 'control-token', None if token_name else secrets.token_hex(32).encode())
    if len(key) < 32:
        raise StartupError('The pairing encryption key must contain at least 32 random bytes.')
    bearer = token.strip()
    if len(bearer) < 32 or not bearer.isascii() or b'\r' in bearer or b'\n' in bearer:
        raise StartupError('The pairing control token must contain at least 32 ASCII characters on one line.')
    if key_path == token_path or key == token:
        raise StartupError('Pairing encryption and control authorization need separate secret files.')
    state = _directory(source.get('TANGLED_PAIRING_STATE_DIR', str(control / 'state')), private=True)
    provider.update(TANGLED_PAIRING_URL=control_url, TANGLED_PAIRING_TOKEN_FILE=str(token_path))
    broker = dict(source, TANGLED_PAIRING_PORT=str(control_port), TANGLED_SECRET_FILE=str(key_path),
        TANGLED_PAIRING_TOKEN_FILE=str(token_path), TANGLED_PAIRING_STATE_DIR=str(state), TANGLED_HOST_URL=host_url)
    return provider, broker


async def _stop(process, timeout):
    if process is None or process.returncode is not None:
        return
    try:
        process.terminate()
    except ProcessLookupError:
        pass
    try:
        await asyncio.wait_for(process.wait(), timeout)
    except asyncio.TimeoutError:
        try:
            process.kill()
        except ProcessLookupError:
            pass
        await process.wait()


async def supervise(provider_command=None, broker_command=None, environment=None, shutdown_timeout=30):
    provider_env, broker_env = prepare_environment(os.environ if environment is None else environment)
    if broker_env is None and provider_env.get('TANGLED_PAIRING_URL'):
        print('Hosted pairing is disabled with retained source credentials; re-enable TANGLED_HOSTED_PAIRING=1 before credential erasure can complete.', file=sys.stderr)
    provider_command = provider_command or ['/usr/local/bin/tangled-provider']
    broker_command = broker_command or [sys.executable, str(Path(__file__).with_name('server.py'))]
    loop = asyncio.get_running_loop()
    stopped = asyncio.Event()
    previous = {}
    for signum in (signal.SIGTERM, signal.SIGINT):
        previous[signum] = signal.getsignal(signum)
        loop.add_signal_handler(signum, stopped.set)
    provider = broker = None
    waiting = []
    try:
        provider = await asyncio.create_subprocess_exec(*provider_command, env=provider_env,
            stdin=asyncio.subprocess.DEVNULL)
        if broker_env is not None:
            broker = await asyncio.create_subprocess_exec(*broker_command, env=broker_env,
                stdin=asyncio.subprocess.DEVNULL)
        signal_waiter = asyncio.create_task(stopped.wait())
        waiting = [signal_waiter, asyncio.create_task(provider.wait())]
        if broker is not None:
            waiting.append(asyncio.create_task(broker.wait()))
        done, _ = await asyncio.wait(waiting, return_when=asyncio.FIRST_COMPLETED)
        if signal_waiter in done:
            return 0
        failed = next(task.result() for task in waiting[1:] if task in done)
        if broker is None:
            return failed if failed >= 0 else 128 - failed
        print('A hosted service child exited unexpectedly; stopping provider and pairing broker.', file=sys.stderr)
        return (failed if failed > 0 else 128 - failed) if failed else 1
    except OSError:
        raise StartupError('Cannot start the provider or pairing runtime; verify the installed image and executable paths.') from None
    finally:
        # Broker gets time to stop source workers and persist checkpoints before
        # the provider disappears. A failed child never leaves its sibling alive.
        await _stop(broker, shutdown_timeout)
        await _stop(provider, shutdown_timeout)
        for task in waiting:
            task.cancel()
        if waiting:
            await asyncio.gather(*waiting, return_exceptions=True)
        for signum, handler in previous.items():
            loop.remove_signal_handler(signum)
            signal.signal(signum, handler)


def main():
    try:
        return asyncio.run(supervise(sys.argv[1:] or None))
    except StartupError as exc:
        print('Hosted pairing startup failed: ' + str(exc), file=sys.stderr)
        return 1
    except Exception:
        print('Hosted pairing supervisor failed before coherent shutdown; inspect the container runtime.', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
