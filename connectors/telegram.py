"""Telegram user-account login, complete available history, and ongoing polling."""
import asyncio
import getpass
import json
import logging
import os
import sys

from common import (ConnectorError, Destination, Vault, MAX_MEDIA, batch, check_account,
                    imported_file)


def map_message(chat_id, raw, media=None, filename=None, media_type=None):
    native_id = f'{chat_id}:{raw["id"]}'
    sender = raw.get('from_id') or {}
    sender = next((str(sender[k]) for k in ('user_id', 'channel_id', 'chat_id') if k in sender), None)
    files = []
    if media is not None:
        # Scope names by message ID, not an untrusted source filename.
        name = f'telegram/{chat_id}/{raw["id"]}/media'
        files.append(imported_file(name, media_type, media))
    item = dict(id=native_id, kind='message', conversation=str(chat_id), sender=sender,
                timestamp=raw.get('date'), text=raw.get('message'),
                attachments=[f['name'] for f in files], data=raw)
    return item, files


def map_contact(user, contact):
    identity = str(user['id'])
    display = ' '.join(value for value in (user.get('first_name'), user.get('last_name')) if value)
    return dict(id=identity, kind='contact', conversation=None, sender=identity, timestamp=None,
                text=display or user.get('username'), attachments=[], data={'user': user, 'contact': contact})


async def source_call(operation):
    from telethon.errors import FloodWaitError, RPCError
    for attempt in range(5):
        try:
            return await asyncio.wait_for(operation(), timeout=120)
        except FloodWaitError as exc:
            if exc.seconds > 300:
                raise ConnectorError('Telegram requires a wait over 5 minutes; restart later') from None
            delay = max(1, exc.seconds)
        except (ConnectionError, OSError, asyncio.TimeoutError):
            delay = 2 ** attempt
        except RPCError:
            raise ConnectorError('Telegram denied a source request; checkpoint unchanged') from None
        if attempt == 4:
            raise ConnectorError('Telegram unavailable after 5 attempts')
        await asyncio.sleep(delay)


async def bounded_media(client, message):
    def check_limit(current, total):
        if current > MAX_MEDIA:
            raise ConnectorError('Telegram media exceeded configured byte limit during download')
    return await source_call(lambda: client.download_media(message, file=bytes, progress_callback=check_limit))


async def run(mode='run'):
    from telethon import TelegramClient
    from telethon.sessions import StringSession
    vault = Vault.from_env('telegram')
    lock = vault.lock()
    credentials = vault.load('auth')
    api_id = int(os.environ['TELEGRAM_API_ID'])
    api_hash = os.environ.get('TELEGRAM_API_HASH')
    if not api_hash:
        from common import private_file
        api_hash = private_file(os.environ['TELEGRAM_API_HASH_FILE']).decode().strip()
    client = TelegramClient(StringSession(credentials.get('session', '')), api_id, api_hash,
                            flood_sleep_threshold=0, request_retries=2, connection_retries=5,
                            receive_updates=False)
    try:
        await client.connect()
        if not await client.is_user_authorized():
            if mode != 'login':
                raise ConnectorError('Telegram login required: run telegram.py login interactively first')
            await client.start(phone=lambda: input('Telegram phone (international): '),
                               password=lambda: getpass.getpass('Telegram two-step password: '),
                               code_callback=lambda: getpass.getpass('Telegram login code: '))
        vault.save('auth', {'session': client.session.save()})
        me = await client.get_me()
        account = str(me.id)
        if me.bot:
            raise ConnectorError('A Telegram user account is required; bot sessions are unsupported')
        print(f'Telegram source account: {account}', flush=True)
        if mode == 'login':
            return
        destination = Destination.from_env()
        cursor = vault.load(destination.checkpoint_name)
        check_account(account, cursor)
        cursor.setdefault('dialogs', {})
        await asyncio.to_thread(destination.send, batch('telegram', account, warnings=[
            'Imports available Telegram cloud chats. Secret chats, deleted content and edits outside the recent polling overlap are unavailable.']))
        interval = max(15, int(os.environ.get('TANGLED_POLL_SECONDS', '60')))
        while True:
            me = await source_call(client.get_me)
            profile = json.loads(me.to_json())
            await asyncio.to_thread(destination.send, batch('telegram', account, [dict(
                id='profile:' + account, kind='profile', conversation=None, sender=account,
                timestamp=None, text=me.first_name, attachments=[], data=profile)]))
            from telethon.tl.functions.contacts import GetContactsRequest
            try:
                contacts_result = await source_call(lambda: client(GetContactsRequest(hash=0)))
            except ConnectorError:
                await asyncio.to_thread(destination.send, batch('telegram', account,
                    warnings=['Telegram contacts snapshot unavailable; message history import continues.']))
            else:
                contacts = {contact.user_id: contact.to_dict() for contact in getattr(contacts_result, 'contacts', [])}
                for user in getattr(contacts_result, 'users', []):
                    raw_user = json.loads(user.to_json())
                    item = map_contact(raw_user, contacts.get(user.id, {}))
                    await asyncio.to_thread(destination.send, batch('telegram', account, [item]))
            # Re-enumerate dialogs so newly joined chats are imported too. This sends no read receipts.
            async for dialog in client.iter_dialogs():
                chat = str(dialog.id)
                previous = int(cursor['dialogs'].get(chat, 0))
                # Overlap catches edits to recent messages without risking the forward cursor.
                minimum = max(0, previous - 100) if previous else 0
                async for message in client.iter_messages(dialog.entity, min_id=minimum, reverse=True,
                                                           wait_time=1):
                    raw = json.loads(message.to_json())
                    files, warnings, media = [], [], None
                    info = message.file
                    if info and info.size is not None and info.size > MAX_MEDIA:
                        warnings.append(f'Media over configured byte limit: {chat}:{message.id}')
                    elif info:
                        media = await bounded_media(client, message)
                        if media is not None and len(media) > MAX_MEDIA:
                            raise ConnectorError('Telegram media exceeded configured byte limit during download')
                    item, files = map_message(dialog.id, raw, media,
                                              info.name or f'media{info.ext or ""}' if info else None,
                                              info.mime_type if info else None)
                    await asyncio.to_thread(destination.send, batch('telegram', account, [item], files, warnings))
                    cursor['dialogs'][chat] = max(previous, message.id, int(cursor['dialogs'].get(chat, 0)))
                    vault.save(destination.checkpoint_name, cursor)
            if mode == 'once':
                return
            await asyncio.sleep(interval)
    finally:
        await client.disconnect()
        lock.close()


if __name__ == '__main__':
    logging.basicConfig(level=logging.CRITICAL)
    try:
        mode = sys.argv[1] if len(sys.argv) > 1 else 'run'
        if mode not in ('login', 'run', 'once'):
            raise ConnectorError('Usage: telegram.py [login|run|once]')
        asyncio.run(run(mode))
    except KeyboardInterrupt:
        pass
    except (ConnectorError, KeyError, ValueError) as exc:
        print(str(exc) if isinstance(exc, ConnectorError) else 'Invalid or missing connector configuration', file=sys.stderr)
        sys.exit(1)
    except Exception:
        print('Telegram connector stopped after a source failure; restart to resume from acknowledged checkpoints', file=sys.stderr)
        sys.exit(1)
