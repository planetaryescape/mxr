#!/usr/bin/env python3
"""Probe owner collisions using two independently initialized fake-provider daemons."""
import argparse
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import time
import uuid

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--runtime', type=Path, required=True)
parser.add_argument('--baseline', action='store_true')
args = parser.parse_args()
root = args.runtime.resolve()
root.mkdir(parents=True, exist_ok=False)
binary = str(args.binary.resolve())
processes = []
records = []

class Daemon:
    def __init__(self, name):
        self.root = root / name
        for directory in ['home', 'tmp', 'config', 'data', 'spool']:
            (self.root / directory).mkdir(parents=True)
        self.sock = '/tmp/mxr-owner-' + uuid.uuid4().hex[:12] + '.sock'
        (self.root / 'config/config.toml').write_text('''[general]
sync_interval = 3600
[search.semantic]
enabled = false
auto_download_models = false
[bridge]
enabled = false
[llm]
enabled = false
[accounts.synthetic]
name = "Synthetic owner fixture"
email = "synthetic@example.com"
is_default = true
[accounts.synthetic.sync]
type = "fake"
[accounts.synthetic.send]
type = "fake"
''')
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('MXR_') and k not in ['HOME', 'TMPDIR', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME']}
        self.env.update(HOME=str(self.root / 'home'), TMPDIR=str(self.root / 'tmp') + '/', MXR_CONFIG_DIR=str(self.root / 'config'), MXR_DATA_DIR=str(self.root / 'data'), MXR_SOCKET_PATH=self.sock, MXR_INSTANCE='owner-' + name + uuid.uuid4().hex[:8], MXR_ACTIVITY='off', MXR_FAKE_DATASET='small', MXR_FAKE_SPOOL_DIR=str(self.root / 'spool'))
        with (self.root / 'daemon.log').open('wb') as log:
            self.process = subprocess.Popen([binary, 'daemon', '--foreground', '--no-bridge'], env=self.env, stdout=log, stderr=log)
        processes.append(self.process)
        for _ in range(200):
            if self.process.poll() is not None:
                raise RuntimeError((self.root / 'daemon.log').read_text())
            if Path(self.sock).exists() and (self.root / 'data/daemon.pid').exists():
                break
            time.sleep(.05)
        else:
            raise TimeoutError('fixture startup')
        self.owner = self.call({'cmd': 'GetArchiveWorkspace'})['data']['workspace']
    def call(self, request, owner=None):
        frame = {'expected_workspace': owner, 'id': 1, 'source': 'cli', 'payload': {'type': 'Request', **request}}
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(30)
            connection.connect(self.sock)
            payload = json.dumps(frame).encode()
            connection.sendall(struct.pack('>I', len(payload)) + payload)
            def read(length):
                result = b''
                while len(result) < length:
                    part = connection.recv(length - len(result))
                    if not part:
                        raise RuntimeError('fixture socket closed')
                    result += part
                return result
            response = json.loads(read(struct.unpack('>I', read(4))[0]))['payload']
        records.append({'daemon': self.root.name, 'command': request['cmd'], 'response': response})
        return response
    def ok(self, request):
        response = self.call(request, None if args.baseline else self.owner)
        assert response['status'] == 'Ok', response
        return response['data']
    def cli(self, *arguments):
        result = subprocess.run([binary, *arguments], env=self.env, text=True, capture_output=True, timeout=30)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)
    def spool_count(self):
        return len(list((self.root / 'spool').glob('accepted-*.json')))

try:
    a, b = Daemon('a'), Daemon('b')
    assert a.owner != b.owner, (a.owner, b.owner)
    created = a.cli('compose', '--to', 'recipient@example.com', '--subject', 'Owner collision', '--body', 'private original text', '--draft', '--no-signature', '--format', 'json')
    draft = a.ok({'cmd': 'GetDraft', 'draft_id': created['draft_id']})['draft']
    # B's configured account is deliberately mapped onto the same account UUID.
    accounts = b.call({'cmd': 'ListAccounts'})['data']['accounts']
    assert accounts[0]['account_id'] == draft['account_id'], (accounts, draft['account_id'])
    copied = copy.deepcopy(draft)
    copied['revision'] = None
    b.ok({'cmd': 'SaveDraft', 'draft': copied})
    payload = b'immutable private attachment bytes'
    metadata = {'id': str(uuid.uuid4()), 'draft_id': draft['id'], 'account_id': draft['account_id'], 'filename': 'private.txt', 'mime_type': 'text/plain', 'length_bytes': len(payload), 'sha256': hashlib.sha256(payload).hexdigest()}
    for daemon in [a, b]:
        daemon.ok({'cmd': 'BeginDraftAttachment', 'metadata': metadata})
        daemon.ok({'cmd': 'WriteDraftAttachmentChunk', 'draft_id': draft['id'], 'account_id': draft['account_id'], 'resource_id': metadata['id'], 'offset': 0, 'data_base64': base64.b64encode(payload).decode()})
        daemon.ok({'cmd': 'FinalizeDraftAttachment', 'draft_id': draft['id'], 'account_id': draft['account_id'], 'resource_id': metadata['id']})
        current = daemon.ok({'cmd': 'GetDraft', 'draft_id': draft['id']})['draft']
        current['managed_attachments'] = [metadata]
        daemon.ok({'cmd': 'UpdateDraft', 'draft': current})
    reviewed = a.ok({'cmd': 'GetDraft', 'draft_id': draft['id']})['draft']
    collision = b.ok({'cmd': 'GetDraft', 'draft_id': draft['id']})['draft']
    assert {k: v for k, v in reviewed.items() if k not in ['created_at', 'updated_at']} == {k: v for k, v in collision.items() if k not in ['created_at', 'updated_at']}, (reviewed, collision)
    before = b.spool_count()
    request = {'cmd': 'SendStoredDraft', 'draft_id': draft['id'], 'managed_attachments_reviewed': True, 'send_identity': {'operation_id': draft['id'] + ':' + str(reviewed['revision']), 'expected_revision': reviewed['revision']}}
    response = b.call(request, None if args.baseline else a.owner)
    if args.baseline:
        assert response['status'] == 'Ok', response
        assert b.spool_count() == before + 1
        result = 'reproduced: reviewed A command sent the identical B draft'
    else:
        assert response['status'] == 'Error' and response['code'] == 'workspace_conflict', response
        assert b.spool_count() == before
        result = 'blocked: stale A command had zero B provider effects'
    print(json.dumps({'result': result, 'owners': [a.owner, b.owner], 'draft_id': draft['id'], 'revision': reviewed['revision'], 'b_spool_before': before, 'b_spool_after': b.spool_count()}, indent=2))
finally:
    for process in processes:
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            process.wait(timeout=15)
    (root / 'evidence.json').write_text(json.dumps(records, indent=2))
