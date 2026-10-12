#!/usr/bin/env python3
"""Prove offline restore with synthetic mail, immutable bytes and real consuming clients."""
import argparse,base64,hashlib,json,os,select,shutil,signal,socket,sqlite3,struct,subprocess,time,uuid
from pathlib import Path
from datetime import datetime,timezone,timedelta
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--released-binary',type=Path);p.add_argument('--runtime',type=Path,required=True);p.add_argument('--source',required=True);p.add_argument('--window',type=float,default=65);a=p.parse_args()
binary=a.binary.resolve();root=a.runtime.resolve();root.mkdir(parents=True,exist_ok=False);cases={};processes=[]
def profile(name):
 r=root/name
 for name in ['data','config','home','tmp','spool']:(r/name).mkdir(parents=True)
 env={k:os.environ[k] for k in ['PATH','LANG','LC_ALL','TERM'] if k in os.environ}
 env.update(HOME=str(r/'home'),TMPDIR=str(r/'tmp')+'/',MXR_DATA_DIR=str(r/'data'),MXR_CONFIG_DIR=str(r/'config'),MXR_SOCKET_PATH='/tmp/mxr-s09-'+uuid.uuid4().hex[:10]+'.sock',MXR_INSTANCE='s09-'+uuid.uuid4().hex[:8],MXR_ACTIVITY='off',MXR_KEYCHAIN='off',MXR_DAEMON_AUTOSTART='off',MXR_FAKE_DATASET='small',MXR_FAKE_SPOOL_DIR=str(r/'spool'))
 return r,env
def mcp_profile():return '[agent_surfaces.profiles.mcp]\nsafety_policy="full"\nallowed_accounts=["synthetic"]\nallow_send=true\nallow_destructive=true\n'
def config(sync=True):return '[general]\nsync_interval=3600\n[bridge]\nenabled=false\n[llm]\nenabled=false\n[search.semantic]\nenabled=false\nauto_download_models=false\n[accounts.synthetic]\nname="Synthetic recovery"\nemail="synthetic@example.com"\n'+('[accounts.synthetic.sync]\ntype="fake"\n' if sync else '')+'[accounts.synthetic.send]\ntype="fake"\n'+mcp_profile()
def cli(env,args,ok=True,executable=binary):
 r=subprocess.run([str(executable),*args],env=env,capture_output=True,text=True,timeout=40)
 assert (r.returncode==0)==ok,(args,r.stdout,r.stderr)
 return json.loads(r.stdout) if ok else r.stdout+r.stderr
def rpc(env,command,owner=None,ok=True):
 frame={'id':1,'source':'cli','payload':{'type':'Request',**command}}
 if owner is not None:frame['expected_workspace']=owner
 data=json.dumps(frame).encode()
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(25);s.connect(env['MXR_SOCKET_PATH']);s.sendall(struct.pack('>I',len(data))+data)
  def exact(n):
   b=b''
   while len(b)<n:
    q=s.recv(n-len(b));assert q;b+=q
   return b
  while True:
   r=json.loads(exact(struct.unpack('>I',exact(4))[0]))
   if r['id']==1 and r['payload']['type']=='Response':
    r=r['payload'];assert (r['status']=='Ok')==ok,r;return r if not ok else r['data']
def start(env,exe=binary):
 log=open(Path(env['MXR_DATA_DIR']).parent/'daemon.log','ab');proc=subprocess.Popen([str(exe),'daemon','--foreground','--no-bridge'],env=env,stdout=log,stderr=log);processes.append(proc)
 for _ in range(200):
  if Path(env['MXR_SOCKET_PATH']).exists():
   try:rpc(env,{'cmd':'Ping'});return proc
   except (OSError,AssertionError):pass
  assert proc.poll() is None,'daemon stopped';time.sleep(.05)
 raise AssertionError('daemon readiness deadline')
def stop(proc):
 if proc.poll() is None:proc.send_signal(signal.SIGTERM);proc.wait(timeout=20)
def owner(env):return rpc(env,{'cmd':'GetArchiveWorkspace'})['workspace']
def draft(env,account,subject):
 now=datetime.now(timezone.utc).isoformat();d={'id':str(uuid.uuid4()),'account_id':account,'revision':None,'from':None,'reply_headers':None,'intent':'new','to':[{'name':None,'email':'recipient@example.com'}],'cc':[],'bcc':[],'subject':subject,'body_markdown':'Private synthetic recovery body','attachments':[],'managed_attachments':[],'inline_assets':[],'inline_calendar_reply':None,'created_at':now,'updated_at':now}
 return rpc(env,{'cmd':'SaveDraft','draft':d},owner(env))['draft']
def mcp(env,name,args):
 proc=subprocess.Popen([str(binary),'mcp','serve'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,bufsize=0);processes.append(proc)
 def call(i,method,params):
  proc.stdin.write((json.dumps({'jsonrpc':'2.0','id':i,'method':method,'params':params})+'\n').encode());proc.stdin.flush()
  deadline=time.monotonic()+25
  while time.monotonic()<deadline:
   ready,_,_=select.select([proc.stdout],[],[],max(.01,deadline-time.monotonic()));assert ready
   r=json.loads(proc.stdout.readline())
   if r.get('id')==i:return r
  raise AssertionError('MCP deadline')
 try:
  call(1,'initialize',{'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'s09-synthetic','version':'1'}})
  proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n');proc.stdin.flush()
  r=call(2,'tools/call',{'name':name,'arguments':args});assert not r.get('error') and not r.get('result',{}).get('isError'),r
  return json.loads(r['result']['content'][0]['text'])
 finally:proc.stdin.close();proc.wait(timeout=10)
try:
 source,se=profile('source');(source/'config/config.toml').write_text(config());sp=start(se)
 for _ in range(100):
  if rpc(se,{'cmd':'GetStatus'})['total_messages']>0:break
  time.sleep(.1)
 account=rpc(se,{'cmd':'ListAccounts'})['accounts'][0]['account_id'];old_owner=owner(se);original=draft(se,account,'Original managed draft');second=draft(se,account,'Second held draft')
 payload=b'synthetic immutable resource\n'*321;meta={'id':str(uuid.uuid4()),'draft_id':original['id'],'account_id':account,'filename':'original.bin','mime_type':'application/octet-stream','length_bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest()}
 rpc(se,{'cmd':'BeginDraftAttachment','metadata':meta},old_owner)
 rpc(se,{'cmd':'WriteDraftAttachmentChunk','draft_id':original['id'],'account_id':account,'resource_id':meta['id'],'offset':0,'data_base64':base64.b64encode(payload).decode()},old_owner)
 rpc(se,{'cmd':'FinalizeDraftAttachment','draft_id':original['id'],'account_id':account,'resource_id':meta['id']},old_owner)
 original['managed_attachments']=[meta];original=rpc(se,{'cmd':'UpdateDraft','draft':original},old_owner)['draft']
 scheduled_at=datetime.now(timezone.utc)+timedelta(seconds=20)
 rpc(se,{'cmd':'ScheduleSend','draft_id':second['id'],'expected_revision':second['revision'],'managed_attachments_reviewed':True,'send_at':scheduled_at.isoformat()},old_owner)
 stop(sp);(source/'config/config.toml').write_text(config(False))
 db=sqlite3.connect(source/'data/mxr.db');db.execute("INSERT INTO user_activity(ts,source,action,context_json) VALUES(1,'cli','search','S09_PHYSICAL_ACTIVITY_CANARY_571991')");db.execute("INSERT INTO draft_safety_overrides VALUES('S09_PHYSICAL_TOKEN_CANARY_571991',NULL,'[]',1,NULL)");archive_evidence={'operation_id':'s09-retained-archive','workspace':old_owner,'state':'unknown','items':[{'message_id':'synthetic-retained-message','state':'unknown','reason':'snapshot predates later source effects'}]}
 db.execute("INSERT INTO mutation_jobs(job_id,kind,status,started_at,data_json) VALUES(?, 'recoverable_archive', 'recoverable', 1, ?)",('s09-retained-archive',json.dumps(archive_evidence)));db.commit();db.close()
 bundle=root/'bundle';args=['backup','create','--source-data',str(source/'data'),'--source-config',str(source/'config/config.toml'),'--output',str(bundle)]
 preview=cli(se,args);assert not bundle.exists();created=cli(se,args+['--confirm']);cli(se,['backup','inspect',str(bundle)])
 for f in created['files']:
  data=(bundle/f['path']).read_bytes()
  for canary in [b'S09_PHYSICAL_ACTIVITY_CANARY_571991',b'S09_PHYSICAL_TOKEN_CANARY_571991']:assert canary not in data
 cases['physical_exclusions_and_preview']=True
 # The source accepts an operation after the snapshot; the destination must not infer unsent.
 sp=start(se);result=rpc(se,{'cmd':'SendStoredDraft','draft_id':original['id'],'managed_attachments_reviewed':True,'send_identity':{'operation_id':'s09-source-after-snapshot','expected_revision':original['revision']}},old_owner);stop(sp)
 destination,de=profile('destination');target=destination/'data';cli(se,['backup','restore',str(bundle),'--destination',str(target)]);assert not (target/'mxr.db').exists();cli(se,['backup','restore',str(bundle),'--destination',str(target),'--confirm']);de['MXR_CONFIG_DIR']=str(target);with_profile=(target/'config.toml').read_text()+mcp_profile();(target/'config.toml').write_text(with_profile);dp=start(de)
 status=cli(de,['recovery','status','--format','json']);status=rpc(de,{'cmd':'GetRecoveryStatus'})['recovery'];new_owner=owner(de);assert old_owner['workspace_id']==new_owner['workspace_id'] and old_owner['recovery_generation']!=new_owner['recovery_generation'];assert status['restore_pending'] and len(status['held_drafts'])>=2;assert any(i.get('archive_evidence')==archive_evidence for i in status['retained_intentions'])
 matches=cli(de,['search','label:inbox','--format','json']);assert matches['results'],matches
 message=rpc(de,{'cmd':'ListEnvelopes','label_id':None,'account_id':None,'limit':1,'offset':0})['envelopes'][0]['id'];cached=cli(de,['cat',message,'--format','json']);assert cached['text_plain'],cached
 rpc(de,{'cmd':'GetDraft','draft_id':original['id']},old_owner,False);held=rpc(de,{'cmd':'GetDraft','draft_id':original['id']},new_owner)['draft'];assert held['body_markdown']==original['body_markdown'];assert (target/f"mxr.draft-attachments/{meta['id']}/final/original.bin").read_bytes()==payload
 before=sqlite3.connect(target/'mxr.db').execute('SELECT COUNT(*) FROM scheduled_send_attempts').fetchone()[0];time.sleep(a.window);assert datetime.now(timezone.utc)>=scheduled_at,'Observation must cross the original schedule';assert sqlite3.connect(target/'mxr.db').execute('SELECT COUNT(*) FROM scheduled_send_attempts').fetchone()[0]==before;assert not list((destination/'spool').glob('accepted-*.json'))
 rpc(de,{'cmd':'SendStoredDraft','draft_id':original['id'],'managed_attachments_reviewed':True,'send_identity':{'operation_id':'forbidden-old','expected_revision':held['revision']}},new_owner,False)
 mcp(de,'mxr_recovery_status',{});cases['paused_cached_reads_bytes_no_replay']=True
 cli(de,['recovery','activate','--expected-workspace',json.dumps(new_owner),'--format','json'])
 cli(de,['recovery','activate','--expected-workspace',json.dumps(new_owner),'--source-owner-stopped','--confirm','--format','json'])
 prepare_args=dict(new_owner,draft_id=original['id'],expected_revision=held['revision'],confirm=False);mcp(de,'mxr_recovery_prepare_draft',prepare_args);prepare_args['confirm']=True
 action=mcp(de,'mxr_recovery_prepare_draft',prepare_args)['action'];fresh=action['prepared_draft'];retry=mcp(de,'mxr_recovery_prepare_draft',prepare_args)['action']['prepared_draft'];assert fresh['id']==retry['id']!=original['id']
 stop(dp);(target/'config.toml').write_text(config(False));dp=start(de)
 identity={'operation_id':'s09-new-reviewed-send','expected_revision':fresh['revision']};rpc(de,{'cmd':'SendStoredDraft','draft_id':fresh['id'],'managed_attachments_reviewed':True,'send_identity':identity},new_owner);rpc(de,{'cmd':'SendStoredDraft','draft_id':fresh['id'],'managed_attachments_reviewed':True,'send_identity':identity},new_owner)
 outcome=rpc(de,{'cmd':'GetSendOutcome','operation_id':identity['operation_id'],'account_id':account},new_owner);assert 'accepted' in json.dumps(outcome).lower();assert len(list((destination/'spool').glob('accepted-*.json')))==1;cases['new_review_idempotent_acceptance']=True
 if a.released_binary:
  bridge=root/'selected-bridge.py';bridge.write_text('import socket,sys,threading\ns=socket.socket(socket.AF_UNIX);s.connect(sys.argv[1])\ndef send():\n while True:\n  b=sys.stdin.buffer.read1(65536)\n  if not b:break\n  s.sendall(b)\n try:s.shutdown(socket.SHUT_WR)\n except OSError:pass\nthreading.Thread(target=send,daemon=True).start()\nwhile True:\n b=s.recv(65536)\n if not b:break\n sys.stdout.buffer.write(b);sys.stdout.buffer.flush()\n')
  def selected(env):return env|{'MXR_DAEMON_ADDR':'cmd://python3 '+str(bridge)+' '+env['MXR_SOCKET_PATH']}
  rejected=cli(selected(de),['status','--format','json'],False,a.released_binary.resolve());assert 'protocol' in rejected.lower(),rejected
  legacy,le=profile('legacy');(legacy/'config/config.toml').write_text(config(False));lp=start(le,a.released_binary.resolve());rejected=cli(selected(le),['status','--format','json'],False);assert 'protocol' in rejected.lower(),rejected;stop(lp);cases['published_11_new_12_both_directions']=True
 stop(dp)
 result={'source':a.source,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'cases':cases,'workspace_id':old_owner['workspace_id'],'old_generation':old_owner['recovery_generation'],'new_generation':new_owner['recovery_generation'],'resource_length':len(payload),'resource_sha256':meta['sha256'],'root_acceptance_bundle':'bundle','root_acceptance_destination':'destination/data','all_owned_processes_stopped':True}
 (root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
finally:
 for proc in processes:
  if proc.poll() is None:proc.terminate();proc.wait(timeout=20)
