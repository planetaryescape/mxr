#!/usr/bin/env python3
"""Verify recoverable Archive through CLI, IPC and HTTP against a fake provider."""
import argparse
import concurrent.futures
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import sqlite3
import struct
import subprocess
import time
import uuid
from urllib.error import HTTPError
from urllib.request import Request, urlopen

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--runtime', type=Path, required=True)
p.add_argument('--evidence', type=Path, required=True)
p.add_argument('--source', required=True)
p.add_argument('--port', type=int, default=18415)
a = p.parse_args()
r = a.runtime.resolve()
r.mkdir(parents=True, exist_ok=False)
for name in ['home', 'tmp', 'config', 'data', 'client-data', 'spool']:
    (r / name).mkdir()
shutil.copy2(a.binary, r / 'mxr')
binary = str(r / 'mxr')
binary_hash = hashlib.sha256(Path(binary).read_bytes()).hexdigest()
sock = '/tmp/mxr-s04-' + uuid.uuid4().hex[:10] + '.sock'
token = 'synthetic-s04-token'
(r / 'config/bridge-token').write_text(token + '\n')
(r / 'config/bridge-token').chmod(0o600)
(r / 'config/config.toml').write_text(f'''[general]
sync_interval = 3600
[search.semantic]
enabled = false
auto_download_models = false
[llm]
enabled = false
[bridge]
enabled = true
bind = "127.0.0.1"
port = {a.port}
token_path = "{r / 'config/bridge-token'}"
[accounts.synthetic]
name = "Synthetic S04"
email = "synthetic@example.com"
is_default = true
[accounts.synthetic.sync]
type = "fake"
[accounts.synthetic.send]
type = "fake"
''')
for n in range(16):
    (r / f'spool/probe-{n:02}.msg').write_text(
        f'From: fixture@example.com\nTo: synthetic@example.com\nSubject: Archive fixture {n}\n\nSynthetic content.\n')
env = {k: v for k, v in os.environ.items() if not k.startswith('MXR_') and k not in {
    'HOME', 'TMPDIR', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME'}}
env.update(HOME=str(r / 'home'), TMPDIR=str(r / 'tmp') + '/',
           MXR_INSTANCE='s04-' + uuid.uuid4().hex[:8], MXR_CONFIG_DIR=str(r / 'config'),
           MXR_DATA_DIR=str(r / 'data'), MXR_SOCKET_PATH=sock, MXR_ACTIVITY='off',
           MXR_FAKE_DATASET='small', MXR_FAKE_SPOOL_DIR=str(r / 'spool'))
client_env = env | {'MXR_DATA_DIR': str(r / 'client-data'), 'MXR_DAEMON_ADDR': 'unix://' + sock}
records, cases = [], []
daemon = None
started = time.monotonic()


def cli(*args, check=True, extra_env=None):
    result = subprocess.run([binary, *args], env=client_env | (extra_env or {}),
                            text=True, capture_output=True, timeout=40)
    records.append({'cli': args, 'code': result.returncode, 'stdout': result.stdout,
                    'stderr': result.stderr})
    if check:
        assert result.returncode == 0, records[-1]
    return result


def sql(query, args=(), write=False):
    uri = f'file:{r / "data/mxr.db"}?mode={"rw" if write else "ro"}'
    with sqlite3.connect(uri, uri=True, timeout=5) as connection:
        rows = connection.execute(query, args).fetchall()
        if write:
            connection.commit()
        return rows


def start():
    global daemon
    with (r / 'daemon.log').open('ab') as log:
        daemon = subprocess.Popen([binary, 'daemon', '--foreground', '--bridge-port', str(a.port)],
                                  env=env, stdout=log, stderr=log, start_new_session=True)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        assert daemon.poll() is None, (r / 'daemon.log').read_text()[-6000:]
        pid = r / 'data/daemon.pid'
        if Path(sock).exists() and pid.exists() and pid.read_text().strip() == str(daemon.pid):
            return
        time.sleep(.1)
    raise RuntimeError('Owned daemon did not publish its PID/socket')


def stop(crash=False):
    if daemon is not None and daemon.poll() is None:
        daemon.send_signal(signal.SIGKILL if crash else signal.SIGINT)
        daemon.wait(timeout=25)


def ipc(request, drop=False):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(30)
        stream.connect(sock)
        payload = json.dumps({'id': 41, 'source': 'cli', 'payload': {'type': 'Request', **request}}).encode()
        stream.sendall(struct.pack('>I', len(payload)) + payload)
        if drop:
            # Keep the socket alive until UDS peer authentication and durable
            # acceptance complete; otherwise this tests an undelivered request.
            identity = request['identity']
            key = f"recoverable_archive:{identity['caller_id']}:{identity['operation_id']}"
            deadline = time.monotonic() + 5
            accepted = False
            while time.monotonic() < deadline:
                rows = sql('SELECT data_json FROM mutation_jobs WHERE job_id = ?', (key,))
                if rows and json.loads(rows[0][0])['accepted']:
                    accepted = True
                    break
                time.sleep(.01)
            assert accepted, 'Request did not reach durable acceptance before ACK loss'
            records.append({'ipc': request, 'response_intentionally_not_read': True,
                            'durable_acceptance_observed_before_close': True})
            return None

        def read(n):
            data = b''
            while len(data) < n:
                part = stream.recv(n - len(data))
                assert part, 'Daemon closed response'
                data += part
            return data

        while True:
            frame = json.loads(read(struct.unpack('>I', read(4))[0]))
            response = frame['payload']
            if response.get('type') == 'Response' and frame['id'] == 41:
                records.append({'ipc': request, 'response': response})
                return response
            records.append({'ipc_event_before_response': response})


def data(response):
    assert response.get('status') == 'Ok', response
    return response['data']


def http(path, body=None):
    request = Request(f'http://127.0.0.1:{a.port}/api/v1/mail/archive-operations/{path}',
                      data=None if body is None else json.dumps(body).encode(),
                      headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
    try:
        with urlopen(request, timeout=10) as response:
            result = (response.status, json.load(response))
    except HTTPError as error:
        result = (error.code, json.loads(error.read()))
    records.append({'http': path, 'body': body, 'status': result[0], 'response': result[1]})
    return result


def preview(ids):
    operation = json.loads(cli('archive', *ids, '--dry-run', '--format', 'json').stdout)
    retained = json.loads((r / 'client-data/archive-operations' /
                           (operation['identity']['operation_id'] + '.json')).read_text())
    assert retained == operation
    assert not operation['accepted']
    records.append({'retained_before_submit': retained})
    return operation


def submission(operation):
    return {'cmd': 'SubmitArchive', 'identity': operation['identity'],
            'review_id': operation['review_id'],
            'mutation': {'mutation': 'Archive', 'message_ids': [x['message_id'] for x in operation['items']]}}


def query(operation):
    result = cli('archive-operation', operation['identity']['operation_id'], '--format', 'json')
    return json.loads(result.stdout)


def settled(operation):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        result = query(operation)
        if result['accepted'] and not result['running']:
            return result
        time.sleep(.05)
    raise RuntimeError('Archive did not settle')


def witnesses(operation, phase='accepted'):
    return [json.loads(path.read_text()) for path in (r / 'spool').glob(f'mutation-{phase}-*.json')
            if json.loads(path.read_text())['mutation_id'] == operation['mutation_id']]


def assert_count(operation, accepted, dispatched=None):
    assert len(witnesses(operation)) == accepted, witnesses(operation)
    if dispatched is not None:
        assert len(witnesses(operation, 'dispatched')) == dispatched


def mark(name, **evidence):
    cases.append({'name': name, 'passed': True, **evidence})


try:
    start()
    data(ipc({'cmd': 'SyncNow', 'account_id': None, 'background': False}))
    rows = sql("SELECT id, provider_id FROM messages WHERE provider_id LIKE 'spool-probe-%' ORDER BY provider_id")
    assert len(rows) == 16, rows
    ids = [row[0] for row in rows]
    workspace = http('workspace')
    assert workspace[0] == 200
    empty = json.loads(cli('archive-operation', '--format', 'json').stdout)
    assert empty == []
    op = preview(ids[:2])
    untouched = preview(ids[2:3])
    listed = json.loads(cli('archive-operation', '--format', 'json').stdout)
    assert len(listed) == 2
    assert cli('archive-operation', '--submit', check=False).returncode != 0
    assert_count(op, 0, 0)
    assert_count(untouched, 0, 0)
    mark('json_list_and_explicit_submission_id')

    # The consumer retained the identity before writing this request. It reads no ACK.
    ipc(submission(op), drop=True)
    result = settled(op)
    assert all(x['state'] == 'completed' for x in result['items']), result
    assert_count(op, 2, 2)
    table = cli('archive-operation', op['identity']['operation_id'], '--format', 'table')
    assert f"Undo with: mxr undo {op['mutation_id']}" in table.stdout
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(lambda _: ipc(submission(op)), range(8)))
    for response in results:
        assert data(response)['operation']['identity'] == op['identity']
    settled(op)
    assert_count(op, 2, 2)
    changed = submission(op)
    changed['mutation']['message_ids'] += [ids[2]]
    assert ipc(changed).get('status') == 'Error'
    unsupported = submission(op)
    unsupported['mutation']['mutation'] = 'ReadAndArchive'
    assert ipc(unsupported).get('status') == 'Error'
    reordered = submission(op)
    reordered['mutation']['message_ids'] = list(reversed(reordered['mutation']['message_ids'])) + [ids[0]]
    data(ipc(reordered))
    stop()
    start()
    assert http('workspace') == workspace
    recovered = query(op)
    assert all(x['state'] == 'completed' for x in recovered['items'])
    status, remote = http('query', {'identity': op['identity']})
    assert status == 200 and remote['operation']['identity'] == op['identity']
    data(ipc(submission(op)))
    settled(op)
    assert_count(op, 2, 2)
    assert_count(untouched, 0, 0)
    mark('lost_ack_concurrent_repeat_changed_payload_restart', operation=op['identity'], accepted=2)

    mismatch = submission(op)
    mismatch['identity'] = copy.deepcopy(op['identity'])
    mismatch['identity']['workspace']['workspace_id'] = str(uuid.uuid4())
    assert ipc(mismatch).get('status') == 'Error'
    mismatch['identity']['workspace'] = copy.deepcopy(op['identity']['workspace'])
    mismatch['identity']['workspace']['recovery_generation'] = str(uuid.uuid4())
    assert ipc(mismatch).get('status') == 'Error'
    assert http('query', {'identity': mismatch['identity']})[0] >= 400
    assert_count(op, 2, 2)
    mark('workspace_and_generation_refusal')

    partial = preview(ids[3:5])
    rejected_provider = partial['items'][0]['provider_id']
    (r / 'spool/mutation-reject').write_text(rejected_provider)
    data(ipc(submission(partial)))
    outcome = settled(partial)
    assert sorted(x['state'] for x in outcome['items']) == ['completed', 'not_executed'], outcome
    assert_count(partial, 1, 2)
    (r / 'spool/mutation-reject').unlink()
    data(ipc(submission(partial)))
    settled(partial)
    assert_count(partial, 1, 2)
    mark('partial_batch_with_proved_nonacceptance', operation=partial['identity'])

    uncertain = preview(ids[5:6])
    (r / 'spool/mutation-uncertain').write_text(uncertain['items'][0]['provider_id'])
    data(ipc(submission(uncertain)))
    outcome = settled(uncertain)
    assert outcome['items'][0]['state'] == 'unknown', outcome
    (r / 'spool/mutation-uncertain').unlink()
    data(ipc(submission(uncertain)))
    settled(uncertain)
    assert_count(uncertain, 1, 1)
    mark('provider_acceptance_response_loss_stays_unknown', operation=uncertain['identity'])

    crashed = preview(ids[6:8])
    (r / 'spool/mutation-pause-after-accept').write_text(crashed['items'][0]['provider_id'])
    data(ipc(submission(crashed)))
    deadline = time.monotonic() + 10
    while len(witnesses(crashed)) == 0 and time.monotonic() < deadline:
        time.sleep(.025)
    assert_count(crashed, 1, 1)
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        in_flight = list(pool.map(lambda _: ipc(submission(crashed)), range(4)))
    assert all(data(response)['operation']['running'] for response in in_flight)
    assert_count(crashed, 1, 1)
    stop(crash=True)
    (r / 'spool/mutation-pause-after-accept').unlink()
    start()
    before = query(crashed)
    assert [x['state'] for x in before['items']] == ['unknown', 'pending'], before
    for _ in range(3):
        query(crashed)
    assert_count(crashed, 1, 1)
    data(ipc(submission(crashed)))
    outcome = settled(crashed)
    assert [x['state'] for x in outcome['items']] == ['unknown', 'completed'], outcome
    assert_count(crashed, 2, 2)
    mark('crash_query_is_read_only_and_same_id_resumes_only_pending', operation=crashed['identity'])

    stale = preview(ids[8:9])
    sql('UPDATE messages SET provider_id = ? WHERE id = ?', ('synthetic-rebound', ids[8]), write=True)
    data(ipc(submission(stale)))
    outcome = settled(stale)
    assert outcome['items'][0]['state'] == 'not_executed', outcome
    assert_count(stale, 0, 0)
    mark('stale_provider_binding_refused_before_dispatch')

    failed_acceptance = preview(ids[9:10])
    sql("CREATE TRIGGER root_fail_archive_acceptance BEFORE UPDATE ON mutation_jobs WHEN json_extract(NEW.data_json, '$.accepted') = 1 BEGIN SELECT RAISE(ABORT, 'synthetic acceptance persistence failure'); END", write=True)
    try:
        response = ipc(submission(failed_acceptance))
        assert response.get('status') == 'Error', response
        assert_count(failed_acceptance, 0, 0)
    finally:
        sql('DROP TRIGGER root_fail_archive_acceptance', write=True)
    mark('failed_durable_acceptance_prevents_provider_call')

    failed_receipt = preview(ids[10:11])
    sql("CREATE TRIGGER root_fail_archive_receipt BEFORE UPDATE ON mutation_jobs WHEN NEW.job_id = 'recoverable_archive:" + failed_receipt['identity']['caller_id'] + ':' + failed_receipt['identity']['operation_id'] + "' AND json_extract(NEW.data_json, '$.items[0].state') = 'provider_succeeded' BEGIN SELECT RAISE(ABORT, 'synthetic receipt persistence failure'); END", write=True)
    try:
        data(ipc(submission(failed_receipt)))
        outcome = settled(failed_receipt)
        assert outcome['items'][0]['state'] == 'unknown', outcome
        assert_count(failed_receipt, 1, 1)
    finally:
        sql('DROP TRIGGER root_fail_archive_receipt', write=True)
    data(ipc(submission(failed_receipt)))
    settled(failed_receipt)
    assert_count(failed_receipt, 1, 1)
    mark('failed_receipt_after_acceptance_never_repeats_effect')

    failed_local = preview(ids[12:13])
    sql("CREATE TRIGGER root_fail_archive_local BEFORE DELETE ON message_labels WHEN OLD.message_id = '" + ids[12] + "' BEGIN SELECT RAISE(ABORT, 'synthetic reconciliation failure'); END", write=True)
    try:
        data(ipc(submission(failed_local)))
        outcome = settled(failed_local)
        assert outcome['items'][0]['state'] == 'provider_succeeded', outcome
        assert_count(failed_local, 1, 1)
    finally:
        sql('DROP TRIGGER root_fail_archive_local', write=True)
    data(ipc(submission(failed_local)))
    settled(failed_local)
    assert_count(failed_local, 1, 1)
    mark('provider_success_survives_failed_local_reconciliation_without_repeat')

    # This is a client-only filesystem fault. The daemon still has its separate data directory.
    blocked_data = r / 'blocked-client'
    blocked_data.mkdir()
    (blocked_data / 'archive-operations').write_text('cannot create journal directory here')
    count_before = len(list((r / 'spool').glob('mutation-dispatched-*.json')))
    refused = cli('archive', ids[11], '--yes', '--format', 'json', check=False,
                  extra_env={'MXR_DATA_DIR': str(blocked_data)})
    assert refused.returncode != 0
    assert len(list((r / 'spool').glob('mutation-dispatched-*.json'))) == count_before
    mark('client_retention_failure_prevents_effect')

    count_before = len(list((r / 'spool').glob('mutation-dispatched-*.json')))
    refused_csv = cli('archive', ids[14], '--yes', '--format', 'csv', check=False)
    assert refused_csv.returncode != 0
    assert len(list((r / 'spool').glob('mutation-dispatched-*.json'))) == count_before
    mark('unsupported_output_refused_before_effect')

    (r / 'spool/mutation-reject').write_text(rows[14][1])
    try:
        rejected_cli = cli('archive', ids[14], '--yes', '--format', 'json', check=False)
        assert rejected_cli.returncode != 0, 'No confirmed Archive must not report command success'
        rejected_operation = json.loads(rejected_cli.stdout)
        assert rejected_operation['items'][0]['state'] == 'not_executed'
        assert query(rejected_operation)['identity'] == rejected_operation['identity']
        repeat = cli('archive-operation', rejected_operation['identity']['operation_id'],
                     '--submit', '--format', 'json', check=False)
        assert repeat.returncode != 0
        assert json.loads(repeat.stdout)['identity'] == rejected_operation['identity']
        assert_count(rejected_operation, 0, 1)
    finally:
        (r / 'spool/mutation-reject').unlink()
    mark('synchronous_known_rejection_fails_with_retained_json_receipt')

    (r / 'spool/mutation-uncertain').write_text(rows[15][1])
    try:
        unknown_cli = cli('archive', ids[15], '--yes', '--format', 'json', check=False)
        assert unknown_cli.returncode != 0, 'Unknown Archive must not report confirmed command success'
        unknown_operation = json.loads(unknown_cli.stdout)
        assert unknown_operation['items'][0]['state'] == 'unknown'
        assert query(unknown_operation)['identity'] == unknown_operation['identity']
        assert_count(unknown_operation, 1, 1)
    finally:
        (r / 'spool/mutation-uncertain').unlink()
    mark('synchronous_unknown_fails_but_read_only_query_succeeds')

    receipts = [json.loads(row[0]) for row in sql("SELECT data_json FROM mutation_jobs WHERE kind='recoverable_archive'")]
    mark('retained_operation_count', count=len(receipts))
    health_request = Request(f'http://127.0.0.1:{a.port}/api/v1/health',
                             headers={'Authorization': 'Bearer ' + token})
    with urlopen(health_request, timeout=10) as health_response:
        records.append({'health_status': health_response.status,
                        'health': json.load(health_response)})
finally:
    stop()
    a.evidence.parent.mkdir(parents=True, exist_ok=True)
    a.evidence.write_text(json.dumps({'source': a.source, 'binary': binary,
        'binary_sha256': binary_hash, 'runtime': str(r), 'elapsed_seconds': time.monotonic() - started,
        'cases': cases, 'records': records, 'owned_daemon_stopped': daemon is None or daemon.poll() is not None}, indent=2) + '\n')

print(json.dumps({'passed_cases': len(cases), 'evidence': str(a.evidence), 'binary_sha256': binary_hash}))
