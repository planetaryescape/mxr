#!/usr/bin/env python3
"""Exercise the shipped companion against isolated real mxr binaries and fake mail."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import uuid

parser = argparse.ArgumentParser()
parser.add_argument('--mxr', type=Path, required=True)
parser.add_argument('--mailmerge', type=Path, required=True)
parser.add_argument('--runtime', type=Path, required=True)
parser.add_argument('--evidence', type=Path, required=True)
parser.add_argument('--port', type=int, default=18415)
a = parser.parse_args()
r = a.runtime.resolve()
r.mkdir(parents=True, exist_ok=False)
for name in ['home', 'tmp', 'config', 'data', 'spool', 'state']:
    (r / name).mkdir()
for source, name in [(a.mxr, 'mxr'), (a.mailmerge, 'mxr-mailmerge')]:
    shutil.copy2(source, r / name)
sock = '/tmp/mxr-mm-' + uuid.uuid4().hex[:10] + '.sock'
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
[accounts.synthetic]
name = "Synthetic companion"
email = "synthetic@example.com"
is_default = true
[accounts.synthetic.sync]
type = "fake"
[accounts.synthetic.send]
type = "fake"
''')
env = {k: v for k, v in os.environ.items() if not k.startswith('MXR_') and k not in {'HOME', 'TMPDIR', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME'}}
env.update(HOME=str(r / 'home'), TMPDIR=str(r / 'tmp') + '/', EDITOR='true', MXR_INSTANCE='mm-' + uuid.uuid4().hex[:8], MXR_CONFIG_DIR=str(r / 'config'), MXR_DATA_DIR=str(r / 'data'), MXR_SOCKET_PATH=sock, MXR_ACTIVITY='off', MXR_FAKE_DATASET='small', MXR_FAKE_SPOOL_DIR=str(r / 'spool'))
(r / 'provenance.json').write_text(json.dumps({'socket': sock, 'instance': env['MXR_INSTANCE'], 'port': a.port, 'binaries': {name: hashlib.sha256((r / name).read_bytes()).hexdigest() for name in ['mxr','mxr-mailmerge']}}))
(r / 'subject.txt').write_text('Synthetic companion campaign')
(r / 'body.html').write_text('<p>Synthetic reviewed content</p>')
(r / 'records.json').write_text('[{"to":"recipient@example.com"}]')
# The proxy delegates every operation to the supplied real mxr. It verifies the
# durable manifest before submission, then kills only its owning companion after
# the real CLI returns to simulate a lost client response.
wrapper = r / 'mxr-loss-proxy.py'
wrapper.write_text('''#!/usr/bin/env python3
import json, os, signal, subprocess, sys
from pathlib import Path
r=Path(__file__).parent
args=sys.argv[1:]
submission=args and args[0]=='send' and '--dry-run' not in args
if submission:
    m=json.loads(Path(os.environ['MM_MANIFEST']).read_text())
    row=next(row for row in m['records'] if row.get('draft_id')==args[1])
    review=row['review']
    for flag,key in [('--account','account_id'),('--expected-revision','expected_revision'),('--operation-id','operation_id')]:
        assert args[args.index(flag)+1]==str(review[key]), (flag,row)
    assert row['status'] in ['uncertain','schedule_uncertain'],row
    assert ('--at' in args)==bool(review.get('scheduled_at')),row
    with (r/'checkpoint-witness.jsonl').open('a') as f:
        f.write(json.dumps({'draft_id':row['draft_id'],'status':row['status'],'review':review})+'\\n')
v=subprocess.run([str(r/'mxr'),*args])
if submission and os.environ.get('MM_LOSE_RESPONSE')=='1':
    os.kill(os.getppid(),signal.SIGKILL)
sys.exit(v.returncode)
''')
wrapper.chmod(0o700)
records = []
daemon = None
started = time.monotonic()

def run(binary, *args, extra=None, allow_failure=False):
    child_env = env.copy()
    child_env.update(extra or {})
    result = subprocess.run([str(binary), *map(str,args)], env=child_env, capture_output=True, text=True, timeout=40)
    records.append({'binary': Path(binary).name, 'args': list(map(str,args)), 'returncode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
    assert allow_failure or result.returncode == 0, records[-1]
    return result

def cli(*args):
    return run(r / 'mxr', *args)

def start():
    global daemon
    with (r / 'daemon.log').open('ab') as log:
        daemon = subprocess.Popen([str(r / 'mxr'), 'daemon', '--foreground', '--bridge-port', str(a.port)], env=env, stdout=log, stderr=log)
    for _ in range(100):
        assert daemon.poll() is None, (r / 'daemon.log').read_text()
        pid = r / 'data/daemon.pid'
        if Path(sock).exists() and pid.exists() and pid.read_text().strip() == str(daemon.pid):
            status = json.loads(cli('status','--format','json').stdout)
            assert status['daemon_pid'] == daemon.pid
            return
        time.sleep(.1)
    raise AssertionError('Owned daemon did not start')

def stop():
    if daemon and daemon.poll() is None:
        daemon.send_signal(signal.SIGINT)
        daemon.wait(timeout=20)

def count():
    return len(list((r / 'spool').glob('accepted-*.json')))

def manifest(campaign):
    return r / 'state' / (campaign + '.json')

def row(campaign):
    return json.loads(manifest(campaign).read_text())['records'][0]

def draft(campaign):
    run(r / 'mxr-mailmerge', 'draft', '--yes', '--account','synthetic','--subject-template',r/'subject.txt','--html-template',r/'body.html','--data',r/'records.json','--campaign-id',campaign,'--state-dir',r/'state','--mxr-binary',r/'mxr')
    assert row(campaign)['status']=='drafted'

def send(campaign, *args, loss=False):
    return run(r/'mxr-mailmerge','send',campaign,'--yes','--state-dir',r/'state','--mxr-binary',wrapper,*args,extra={'MM_MANIFEST':str(manifest(campaign)),'MM_LOSE_RESPONSE':'1' if loss else '0'},allow_failure=loss)

try:
    start()
    draft('immediate')
    send('immediate')
    assert row('immediate')['status']=='sent' and count()==1
    draft('lost-immediate')
    send('lost-immediate',loss=True)
    retained=row('lost-immediate')
    assert retained['status']=='uncertain' and count()==2
    operation=retained['review']['operation_id']
    account=retained['review']['account_id']
    receipt=json.loads(cli('send-outcome',operation,'--account',account,'--format','json').stdout)
    assert receipt['state']=='accepted'
    # Response recovery must not depend on a live draft. Wait only for cleanup.
    for _ in range(50):
        db=sqlite3.connect(r/'data/mxr.db')
        exists=db.execute('SELECT count(*) FROM drafts WHERE id=?',(retained['draft_id'],)).fetchone()[0]
        db.close()
        if not exists: break
        time.sleep(.1)
    assert not exists
    stop(); start()
    send('lost-immediate','--retry-failed')
    assert row('lost-immediate')['status']=='sent' and row('lost-immediate')['review']['operation_id']==operation and count()==2
    draft('rejected')
    (r/'spool/send-reject').write_text('synthetic')
    send('rejected')
    rejected=row('rejected')
    assert rejected['status']=='failed' and count()==2
    (r/'spool/send-reject').unlink()
    send('rejected')
    assert row('rejected')['status']=='failed' and count()==2
    send('rejected','--retry-failed')
    assert row('rejected')['status']=='sent' and row('rejected')['review']['operation_id']!=rejected['review']['operation_id'] and count()==3
    draft('unknown')
    (r/'spool/send-uncertain').write_text('synthetic')
    send('unknown')
    unknown=row('unknown')
    assert unknown['status']=='uncertain'
    uncertain_count=count()
    (r/'spool/send-uncertain').unlink()
    stop(); start()
    send('unknown','--retry-failed')
    assert row('unknown')['status']=='uncertain' and row('unknown')['review']==unknown['review'] and count()==uncertain_count
    draft('scheduled')
    send('scheduled','--at','in 2h')
    scheduled=row('scheduled')
    assert scheduled['status']=='scheduled' and scheduled['review']['scheduled_at'] and count()==uncertain_count
    draft('lost-schedule')
    send('lost-schedule','--at','in 3h',loss=True)
    lost_schedule=row('lost-schedule')
    assert lost_schedule['status']=='schedule_uncertain'
    stop(); start()
    send('lost-schedule','--retry-failed')
    assert row('lost-schedule')['status']=='schedule_uncertain' and row('lost-schedule')['review']==lost_schedule['review'] and count()==uncertain_count
    db=sqlite3.connect(r/'data/mxr.db')
    schedule_rows=db.execute('SELECT id,revision,scheduled_send_revision,send_at FROM drafts WHERE id IN (?,?)',(scheduled['draft_id'],lost_schedule['draft_id'])).fetchall()
    db.close()
    assert len(schedule_rows)==2 and all(revision==claimed==1 and at for _,revision,claimed,at in schedule_rows),schedule_rows
    evidence={'status':'passed','wall_seconds':time.monotonic()-started,'provider_acceptances':count(),'binaries':json.loads((r/'provenance.json').read_text())['binaries'],'manifests':{c:json.loads(manifest(c).read_text()) for c in ['immediate','lost-immediate','rejected','unknown','scheduled','lost-schedule']},'schedule_rows':schedule_rows,'checkpoint_witnesses':[json.loads(v) for v in (r/'checkpoint-witness.jsonl').read_text().splitlines()],'commands':records}
    a.evidence.parent.mkdir(parents=True,exist_ok=True)
    a.evidence.write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps({k:evidence[k] for k in ['status','wall_seconds','provider_acceptances','binaries']}))
finally:
    stop()
