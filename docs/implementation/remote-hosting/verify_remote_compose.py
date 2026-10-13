#!/usr/bin/env python3
"""Synthetic native-client reply/send journey; caller supplies private HTTPS forwarding.

Host controls are methods so the same assertions can use a remote Linux fixture.
No real mailbox or provider is used. Evidence excludes credentials and mail bodies.
"""
import argparse
import base64
import hashlib
import fcntl
import os
import pty
import re
import struct
import termios
import json
from pathlib import Path
import select
import sqlite3
import subprocess
import time
import uuid

from verify_remote_read import RemoteReadFixture


class RemoteComposeFixture(RemoteReadFixture):
    def __init__(self, args):
        super().__init__(args)
        self.spool = self.root / 'spool'
        self.spool.mkdir(mode=0o700)
        self.env['MXR_FAKE_SPOOL_DIR'] = str(self.spool)
        self.clients = {name: {**env, 'MXR_FAKE_SPOOL_DIR': str(self.spool)}
                        for name, env in self.clients.items()}

    def spool_records(self):
        return [json.loads(p.read_text()) for p in sorted(self.spool.glob('accepted-*.json'))]

    def spool_control(self, name, enabled):
        if name not in {'send-pause-after-accept', 'send-uncertain', 'send-reject'}:
            raise ValueError('Unsupported synthetic spool control')
        path = self.spool / name
        if enabled:
            path.touch()
        else:
            path.unlink(missing_ok=True)

    def host_counts(self):
        with sqlite3.connect(self.root / 'data/mxr.db') as db:
            return {key: db.execute(query).fetchone()[0] for key, query in (
                ('drafts', 'SELECT COUNT(*) FROM drafts'),
                ('resources', 'SELECT COUNT(*) FROM draft_attachment_resources'),
                ('safety_runs', 'SELECT COUNT(*) FROM draft_safety_runs'),
                ('safety_overrides', 'SELECT COUNT(*) FROM draft_safety_overrides'))}

    def received_attachment_hashes(self):
        return sorted(self.request({'cmd': 'GetAttachmentTransferMetadata', 'message_id': self.message_id, 'attachment_id': a['id']})['data']['metadata']['sha256']
                      for a in self.host_body['attachments'])

    def restart_host(self):
        self.stop()
        self.start()

    def upgrade(self, name):
        env = self.clients[name]
        old = self.state(name)['selected']['client_id']
        arguments = ('connect', self.origin, '--name', 'Synthetic send client ' + name, '--allow-send')
        preview = self.command(env, *arguments, '--dry-run')
        self.check('Send-grant preview is explicit and preserves selection',
                   preview['requested_capabilities'] == ['mail_read', 'mail_send']
                   and self.state(name)['selected']['client_id'] == old)
        pending = self.command(env, *arguments, '--confirm')['pairing']
        self.check('Upgrade stays pending while old read registration works',
                   pending['capabilities'] == ['mail_read', 'mail_send']
                   and self.state(name)['selected']['client_id'] == old)
        self.read_mail(name)
        self.command(env, *arguments, '--confirm', ok=False)
        self.check('Unapproved upgrade preserves old selection', self.state(name)['selected']['client_id'] == old)
        self.command(self.env, 'serve', 'pair', 'approve', pending['request_id'], '--dry-run')
        self.command(env, *arguments, '--confirm', ok=False)
        self.command(self.env, 'serve', 'pair', 'approve', pending['request_id'], '--confirm')
        selected = self.command(env, *arguments, '--confirm')
        self.check('Approved upgrade commits a distinct send registration',
                   selected['client_id'] != old and selected['capabilities'] == ['mail_read', 'mail_send'])
        records = self.remote('Clients')['clients']
        self.check('Upgrade retires exactly its old registration',
                   next(c for c in records if c['client_id'] == old)['revoked_at'] is not None)
        return selected['client_id']

    def reply_draft(self, name, body, attachment=None):
        args = ['reply', self.message_id, '--body', body, '--draft', '--no-signature']
        if attachment:
            args.extend(['--attach', str(attachment)])
        saved = self.command(self.clients[name], *args)
        return self.command(self.clients[name], 'send', saved['draft_id'], '--dry-run')

    def send_arguments(self, preview, operation=None):
        return ('send', preview['draft']['id'], '--expected-revision',
                str(preview['draft']['revision']), '--account', preview['draft']['account_id'], '--expected-workspace',
                json.dumps(preview['expected_workspace']), '--operation-id',
                operation or preview['send_identity']['operation_id'])

    def outcome(self, name, preview, operation=None):
        return self.command(self.clients[name], 'send-outcome',
                            operation or preview['send_identity']['operation_id'],
                            '--account', preview['draft']['account_id'],
                            '--expected-workspace', json.dumps(preview['expected_workspace']))

    def wait_outcome(self, name, preview, state):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            outcome = self.outcome(name, preview)
            if outcome and outcome['state'] == state:
                return outcome
            time.sleep(.1)
        raise AssertionError('Durable outcome deadline: ' + state)

    def mcp_reply(self, name):
        process = subprocess.Popen([str(self.binary), 'mcp', 'serve'], env=self.clients[name],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL, bufsize=0)
        number = 0
        def call(method, params):
            nonlocal number
            number += 1
            process.stdin.write((json.dumps({'jsonrpc': '2.0', 'id': number,
                                            'method': method, 'params': params}) + '\n').encode())
            process.stdin.flush()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                ready, _, _ = select.select([process.stdout], [], [], .1)
                if ready:
                    response = json.loads(process.stdout.readline())
                    if response.get('id') == number:
                        if response.get('error') or response.get('result', {}).get('isError'):
                            raise AssertionError('MCP call failed: ' + method + ': ' + json.dumps(response))
                        return response['result']
            raise AssertionError('MCP deadline')
        def tool(name, args):
            result = call('tools/call', {'name': name, 'arguments': args})
            return json.loads(next(c['text'] for c in result['content'] if c['type'] == 'text'))
        try:
            call('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {}, 'clientInfo': {'name': 'remote-compose-fixture', 'version': '1'}})
            process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
            process.stdin.flush()
            prepared = tool('mxr_prepare_reply', {'message_id': self.message_id, 'body': 'MCP synthetic reply.'})
            owner = prepared['expected_workspace']
            saved = tool('mxr_save_draft', {'draft': prepared['draft'], 'expected_workspace': owner})['draft']
            payload = b'MCP attachment bytes\x00\xff'
            metadata = {'id': str(uuid.uuid4()), 'draft_id': saved['id'], 'account_id': saved['account_id'],
                        'filename': 'mcp.bin', 'mime_type': 'application/octet-stream',
                        'length_bytes': len(payload), 'sha256': hashlib.sha256(payload).hexdigest()}
            common = {'expected_workspace': owner, 'draft_id': saved['id'], 'account_id': saved['account_id'], 'resource_id': metadata['id']}
            tool('mxr_draft_attachment_begin', {'metadata': metadata, 'expected_workspace': owner, 'confirm': True})
            tool('mxr_draft_attachment_chunk', {**common, 'offset': 0, 'data_base64': base64.b64encode(payload).decode(), 'confirm': True})
            tool('mxr_draft_attachment_finalize', {**common, 'confirm': True})
            saved['managed_attachments'] = [metadata]
            saved = tool('mxr_update_draft', {'draft': saved, 'expected_workspace': owner})['draft']
            preview = tool('mxr_draft_preview', {'draft': saved, 'expected_workspace': owner})
            identity = preview['send_identity']
            before = len(self.spool_records())
            tool('mxr_send_draft', {'draft_id': saved['id'], 'operation_id': identity['operation_id'], 'expected_revision': identity['expected_revision'], 'expected_workspace': owner, 'managed_attachments_reviewed': True, 'confirm': True})
            outcome = tool('mxr_send_outcome', {'account_id': saved['account_id'], 'operation_id': identity['operation_id'], 'expected_workspace': owner})['outcome']
            self.check('Actual MCP prepare, upload, preview, confirmed send and recovery journey', outcome['state'] == 'accepted' and len(self.spool_records()) == before + 1)
        finally:
            process.stdin.close()
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)

    def tui_reply(self, name, reconnect=False):
        attachment = self.root / ('client-' + name) / 'tui-attachment.bin'
        attachment.write_bytes(b'TUI client attachment bytes')
        editor = self.root / ('client-' + name) / 'fixture-editor.py'
        editor.write_text("#!/usr/bin/env python3\nfrom pathlib import Path\nimport sys, re\np=Path(sys.argv[-1])\ns=p.read_text()\ns=re.sub(r'^attach:.*$', 'attach: [" + json.dumps(str(attachment)) + "]', s, flags=re.M)\np.write_text(s + '\\nTUI synthetic reply.\\n')\n")
        editor.chmod(0o700)
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 42, 140, 0, 0))
        env = {**self.clients[name], 'TERM': 'xterm-256color', 'EDITOR': str(editor)}
        process = subprocess.Popen([str(self.binary)], env=env, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
        os.close(slave)
        transcript = bytearray()
        status_queries = 0
        def rendered(text, timeout=20):
            nonlocal status_queries
            start = len(transcript)
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                ready, _, _ = select.select([master], [], [], .1)
                if ready:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        break
                    transcript.extend(data)
                    while transcript.count(b'\x1b[5n') > status_queries:
                        os.write(master, b'\x1b[0n')
                        status_queries += 1
                    plain = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', transcript[start:].decode(errors='replace'))
                    if text in plain:
                        return True
                if process.poll() is not None:
                    break
            return False
        before = len(self.spool_records())
        try:
            self.check('TUI remote mailbox opens before compose', rendered('Remote mailbox'))
            os.write(master, b'/subject:"Q1 Report review"\r')
            self.check('TUI finds hosted reply target', rendered('Search Results / lexical / showing 1 of'))
            os.write(master, b'\r')
            self.check('TUI reads before replying', rendered('Q1 report body text'))
            os.write(master, b'r')
            self.check('TUI local editor reaches explicit send preview', rendered('[s] send'))
            self.check('TUI preview has not delivered mail', len(self.spool_records()) == before)
            if reconnect:
                counts = self.host_counts()
                self.gateway_stop()
                self.gateway_start()
                # The worker retries failed reconnects every two seconds; the
                # modal obscures its connection status while confirmation stays open.
                time.sleep(3)
                attachment.write_bytes(b'Changed after preview and reconnect')
                os.write(master, b's')
                self.check('Reconnected TUI refuses changed attachment before saving', rendered('changed since preview', timeout=20))
                self.check('Reconnected preview refusal writes no host draft or upload', self.host_counts() == counts)
                self.check('Reconnected preview refusal has no provider effect', len(self.spool_records()) == before)
                return
            os.write(master, b's')
            self.check('TUI confirmed send completes', rendered('Sent!', timeout=30))
            self.check('Actual TUI send delivered exactly once', len(self.spool_records()) == before + 1)
            self.check('TUI send used uploaded local attachment bytes', self.spool_records()[-1]['attachments'][0]['sha256'] == hashlib.sha256(attachment.read_bytes()).hexdigest())
        finally:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)
            os.close(master)
            (self.root / ('tui-compose-' + name + ('-reconnect' if reconnect else '') + '.log')).write_bytes(transcript)

    def run_remote(self):
        ids = {name: self.pair(name) for name in self.clients}
        self.command(self.clients['a'], 'reply', self.message_id, '--body', 'Read-only must fail.', '--draft', '--no-signature', ok=False)
        ids['a'] = self.upgrade('a')
        self.state('a')
        self.read_mail('b')
        directory = self.root / 'client-b' / 'received'
        self.command(self.clients['b'], 'attachments', 'download', self.message_id, '--dir', str(directory))
        downloaded = list(directory.iterdir())
        # Host bytes are accessed only by the owned fixture control, never by the remote client.
        self.check('Read-only client downloads exact received attachment bytes', bool(downloaded) and sorted(hashlib.sha256(p.read_bytes()).hexdigest() for p in downloaded) == self.received_attachment_hashes())
        client_attachment = self.root / 'client-a' / 'local-attachment.bin'
        payload = b'Client-only binary bytes\x00\xff\x10'
        client_attachment.write_bytes(payload)
        before = self.host_counts()
        preview = self.command(self.clients['a'], 'reply', self.message_id, '--body', 'Please see attached.', '--attach', str(client_attachment), '--dry-run', '--no-signature')
        self.check('Attachment preview writes no host state and reveals no client path', self.host_counts() == before and str(client_attachment) not in json.dumps(preview))
        preview = self.reply_draft('a', 'Please see attached.', client_attachment)
        self.check('Saved portable draft includes exact attachment metadata', not preview['draft']['attachments'] and preview['draft']['managed_attachments'][0]['sha256'] == hashlib.sha256(payload).hexdigest())
        self.spool_control('send-pause-after-accept', True)
        env = {**self.clients['a'], 'MXR_IPC_TIMEOUT_SECS': '1'}
        self.command(env, *self.send_arguments(preview), ok=False)
        self.check('Lost response still delivered exactly once', len(self.spool_records()) == 1)
        self.spool_control('send-pause-after-accept', False)
        accepted = self.wait_outcome('a', preview, 'accepted')
        self.check('Provider received exact uploaded bytes', self.spool_records()[0]['attachments'][0]['sha256'] == hashlib.sha256(payload).hexdigest())
        self.restart_host()
        recovered = self.outcome('a', preview)
        self.check('Restart recovers original accepted operation', recovered['identity'] == accepted['identity'] and recovered['state'] == 'accepted')
        replay = self.command(self.clients['a'], *self.send_arguments(preview))
        self.check('CLI accepted operation replay survives deleted draft without delivery', replay['state'] == 'accepted' and len(self.spool_records()) == 1)
        # Outcome recovery works after successful send deletes its draft.
        self.mcp_reply('a')
        self.tui_reply('a', reconnect=True)
        self.tui_reply('a')
        uncertain = self.reply_draft('a', 'Uncertain synthetic reply.')
        self.spool_control('send-uncertain', True)
        self.command(self.clients['a'], *self.send_arguments(uncertain), ok=False)
        self.spool_control('send-uncertain', False)
        count = len(self.spool_records())
        self.check('Unknown acceptance remains explicitly unknown', self.outcome('a', uncertain)['state'] == 'unknown')
        self.restart_host()
        self.command(self.clients['a'], *self.send_arguments(uncertain), ok=False)
        self.command(self.clients['a'], *self.send_arguments(uncertain, 'different-operation'), ok=False)
        self.check('Unknown replay and changed operation never redeliver', len(self.spool_records()) == count)
        rejected = self.reply_draft('a', 'Rejected synthetic reply.')
        self.spool_control('send-reject', True)
        self.command(self.clients['a'], *self.send_arguments(rejected), ok=False)
        self.check('Definite nonacceptance is queryable without delivery', self.outcome('a', rejected)['state'] == 'not_accepted' and len(self.spool_records()) == count)
        self.spool_control('send-reject', False)
        self.command(self.clients['a'], *self.send_arguments(rejected), ok=False)
        self.check('Rejected operation replay does not silently create a new attempt', len(self.spool_records()) == count)
        retry = 'reviewed-retry:' + uuid.uuid4().hex
        self.command(self.clients['a'], *self.send_arguments(rejected, retry))
        self.check('Explicit reviewed new operation succeeds after definite rejection', self.outcome('a', rejected, retry)['state'] == 'accepted' and len(self.spool_records()) == count + 1)
        self.command(self.clients['a'], 'connect', '--local', '--confirm')
        local_log = (self.root / 'client-a' / 'local-daemon.log').open('wb')
        local = subprocess.Popen([str(self.binary), 'daemon', '--foreground'], env=self.clients['a'], stdout=local_log, stderr=local_log, start_new_session=True)
        try:
            deadline = time.monotonic() + 10
            while not Path(self.clients['a']['MXR_SOCKET_PATH']).exists():
                if local.poll() is not None or time.monotonic() >= deadline:
                    raise AssertionError('Owned alternate workspace did not start')
                time.sleep(.1)
            result = self.command(self.clients['a'], 'send-outcome', uncertain['send_identity']['operation_id'], '--account', uncertain['draft']['account_id'], '--expected-workspace', json.dumps(uncertain['expected_workspace']), ok=False)
            self.check('Switch to an owned alternate workspace cannot rebind retained outcome', 'workspace' in result.stderr.lower())
        finally:
            if local.poll() is None:
                local.terminate()
            local.wait(timeout=10)
            local_log.close()
        result = self.command(self.clients['b'], 'search', 'subject:"Q1 Report review"', '--mode', 'lexical')
        rows = result if isinstance(result, list) else result['results']
        self.check('Independent read-only client still sees original mail after sends and deregistration', {r['message_id'] for r in self.mail}.issubset({r['message_id'] for r in rows}))
        self.evidence.update(result='passed', client_ids=ids, acceptance_count=len(self.spool_records()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('binary', 'runtime'):
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--origin', required=True)
    parser.add_argument('--listen', default='127.0.0.1:18741')
    args = parser.parse_args()
    fixture = RemoteComposeFixture(args)
    started = time.monotonic()
    try:
        fixture.prepare()
        fixture.run_remote()
    finally:
        fixture.gateway_stop()
        fixture.stop()
        fixture.evidence['elapsed_seconds'] = round(time.monotonic() - started, 3)
        (fixture.root / 'result.json').write_text(json.dumps(fixture.evidence, indent=2) + '\n')
    print(json.dumps(fixture.evidence), flush=True)


if __name__ == '__main__':
    main()
