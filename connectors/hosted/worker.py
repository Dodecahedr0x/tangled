"""Record process ownership durably before executing a native source connector.

The session ID is not a credential; it lets a restarted broker distinguish its
orphaned process from an unrelated process that later reused the same PID.
"""
import os
from pathlib import Path
import shutil
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import Vault, private_file
from hosted.service import CONNECTORS, WhatsAppVault


def main():
    identity, source, mode = sys.argv[1:]
    if source not in ('telegram', 'twitter', 'whatsapp') or mode not in ('run', 'login'):
        raise ValueError('invalid worker')
    cls = WhatsAppVault if source == 'whatsapp' else Vault
    vault = cls(os.environ['TANGLED_STATE_DIR'], private_file(os.environ['TANGLED_SECRET_FILE']))
    vault.save('hosted-process', {'pid': os.getpid(), 'id': identity, 'source': source})
    if source == 'whatsapp':
        command = [shutil.which(os.environ.get('TANGLED_NODE', 'node')),
            str(CONNECTORS / 'whatsapp/index.mjs'), mode, identity]
    else:
        script = 'telegram.py' if source == 'telegram' else 'x.py'
        command = [sys.executable, str(CONNECTORS / script), mode, identity]
    os.execvpe(command[0], command, os.environ)


if __name__ == '__main__':
    main()
