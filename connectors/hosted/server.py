"""Explicit operator-run loopback pairing service: python connectors/hosted/server.py.

Required: TANGLED_SECRET_FILE (private random encryption key),
TANGLED_PAIRING_TOKEN_FILE (private random control bearer), TANGLED_HOST_URL,
TANGLED_PAIRING_STATE_DIR (0700). Source application credentials stay here.
"""
import asyncio
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import logging
import os
from pathlib import Path
import signal
import sys
import threading

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import ConnectorError, Vault, private_file, validate_destination
from hosted.service import Broker


def control_server(address, broker, token, loop):
    if address[0] not in ('127.0.0.1', '::1'):
        raise ConnectorError('Pairing control must bind a loopback address.')
    gate = asyncio.Lock()

    async def dispatch(path, data):
        async with gate:
            if path == '/availability':
                return broker.availability()
            if not isinstance(data, dict) or not isinstance(data.get('owner'), str) or not 0 < len(data['owner']) <= 512:
                raise ConnectorError('An authenticated owner is required.')
            owner = data['owner']
            if path == '/start':
                return await broker.start(owner, data['source'], data.get('generation', 0))
            if path == '/status':
                return await broker.status(owner, data['id'])
            if path == '/step':
                return await broker.step(owner, data['id'], data['values'])
            if path == '/bind':
                return await broker.bind(owner, data['id'], data['binding'])
            if path == '/callback':
                return await broker.callback(owner, data['state'], data.get('code', ''))
            if path == '/disconnect':
                return await broker.disconnect(owner, data['id'])
            if path == '/stop_scope':
                source, account = data.get('source'), data.get('account')
                if source not in (None, 'telegram', 'twitter', 'whatsapp') or (account is not None and not isinstance(account, str)):
                    raise ConnectorError('Invalid source scope.')
                return await broker.stop_scope(owner, source, account)
            raise ConnectorError('Unknown control operation.')

    class Handler(BaseHTTPRequestHandler):
        server_version = 'TangledPairing'
        sys_version = ''

        def log_message(self, *args):
            pass  # Never log paths, request bodies, source input or capabilities.

        def answer(self, status, result):
            encoded = json.dumps(result, separators=(',', ':')).encode()
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Content-Length', str(len(encoded)))
            self.send_header('X-Content-Type-Options', 'nosniff')
            self.end_headers()
            self.wfile.write(encoded)

        def handle_control(self):
            if not hmac.compare_digest(self.headers.get('Authorization', '').encode(), b'Bearer ' + token):
                self.answer(401, {'error': 'Control authorization required.'})
                return
            if self.headers.get('Origin'):
                self.answer(403, {'error': 'Browser access to pairing control is forbidden.'})
                return
            if self.command == 'GET' and self.path != '/availability':
                self.answer(405, {'error': 'Use the authenticated POST control operation.'})
                return
            try:
                length = int(self.headers.get('Content-Length', '0'))
                if length < 0 or length > 16384:
                    self.answer(413, {'error': 'Control request exceeds limit.'})
                    return
                if self.headers.get('Transfer-Encoding'):
                    raise ValueError('transfer encoding')
                data = json.loads(self.rfile.read(length)) if length else {}
                future = asyncio.run_coroutine_threadsafe(dispatch(self.path, data), loop)
                try:
                    result = future.result(timeout=130)
                except TimeoutError:
                    future.cancel()
                    self.answer(503, {'error': 'Pairing operation timed out; retry status.'})
                    return
                self.answer(200, result)
            except (ConnectorError, ValueError, TypeError, KeyError):
                self.answer(400, {'error': 'Pairing operation rejected. Verify the owner, challenge and configuration.'})
            except Exception:
                self.answer(503, {'error': 'Pairing service unavailable; no completion is acknowledged.'})

        do_GET = handle_control
        do_POST = handle_control

    server = ThreadingHTTPServer(address, Handler)
    server.daemon_threads = True
    return server


async def main():
    logging.basicConfig(level=logging.CRITICAL)
    key = private_file(os.environ['TANGLED_SECRET_FILE'])
    token = private_file(os.environ['TANGLED_PAIRING_TOKEN_FILE']).strip()
    if len(token) < 32 or b'\n' in token or b'\r' in token:
        raise ConnectorError('Pairing bearer must contain at least 32 random bytes on one line.')
    validate_destination(os.environ['TANGLED_HOST_URL'])
    directory = Path(os.environ['TANGLED_PAIRING_STATE_DIR']).expanduser()
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    if directory.is_symlink() or directory.stat().st_mode & 0o077:
        raise ConnectorError('Pairing state must be a private 0700 directory.')
    vault = Vault(directory, key)
    lock = vault.lock()
    broker = Broker(vault)
    server = control_server(('127.0.0.1', int(os.environ.get('TANGLED_PAIRING_PORT', '8791'))),
        broker, token, asyncio.get_running_loop())
    stopped = asyncio.Event()
    loop = asyncio.get_running_loop()
    for signum in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(signum, stopped.set)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    maintenance = asyncio.create_task(broker.maintenance())
    try:
        await broker.resume()
        await stopped.wait()
    finally:
        await asyncio.to_thread(server.shutdown)
        server.server_close()
        maintenance.cancel()
        try:
            await maintenance
        except asyncio.CancelledError:
            pass
        await broker.shutdown()
        lock.close()


if __name__ == '__main__':
    try:
        asyncio.run(main())
    except Exception:
        print('Pairing host failed. Verify private key/token files, loopback port, state directory and source app configuration.', file=sys.stderr)
        sys.exit(1)
