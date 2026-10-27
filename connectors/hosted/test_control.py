import asyncio
import json
from pathlib import Path
import tempfile
import threading
import unittest
from urllib import error, request

from common import Vault
from hosted.server import control_server
from hosted.service import Broker
from hosted.test_hosted import ControlledSource


class ControlTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.broker = Broker(Vault(Path(self.tmp.name), b'k' * 32), {'telegram': ControlledSource()})
        self.server = control_server(('127.0.0.1', 0), self.broker, b'control-secret', asyncio.get_running_loop())
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.base = 'http://127.0.0.1:' + str(self.server.server_port)

    async def asyncTearDown(self):
        await asyncio.to_thread(self.server.shutdown)
        self.server.server_close()
        await self.broker.shutdown()
        self.tmp.cleanup()

    async def call(self, path, data=None, token='control-secret', origin=None):
        def send():
            headers = {'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'}
            if origin:
                headers['Origin'] = origin
            req = request.Request(self.base + path, data=json.dumps(data).encode() if data is not None else None,
                headers=headers)
            try:
                with request.urlopen(req) as response:
                    return response.status, json.load(response)
            except error.HTTPError as exc:
                try:
                    return exc.code, json.load(exc)
                finally:
                    exc.close()
        return await asyncio.to_thread(send)

    async def test_control_requires_bearer_and_rejects_browser_origin(self):
        self.assertEqual((await self.call('/availability', token='wrong'))[0], 401)
        self.assertEqual((await self.call('/availability', origin='https://attacker.example'))[0], 403)
        status, result = await self.call('/availability')
        self.assertEqual(status, 200)
        self.assertTrue(result['sources'][0]['available'])

    async def test_control_owner_isolation_and_malformed_input(self):
        status, item = await self.call('/start', {'owner': 'alice', 'source': 'telegram'})
        self.assertEqual(status, 200)
        self.assertEqual((await self.call('/status', {'owner': 'bob', 'id': item['id']}))[0], 400)
        self.assertEqual((await self.call('/step', {'owner': 'alice', 'id': item['id'], 'values': []}))[0], 400)
        status, item = await self.call('/step', {'owner': 'alice', 'id': item['id'], 'values': {'phone': '+33123456789'}})
        self.assertEqual((status, item['account']), (200, '12345'))
        self.assertEqual((await self.call('/stop_scope', {'owner': 'alice', 'source': 'telegram'}))[1], {'stopped': True})
        self.assertEqual((await self.call('/status', {'owner': 'alice', 'id': item['id']}))[0], 400)


if __name__ == '__main__':
    unittest.main()
