#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact="${MXR_PROOF_ARTIFACT:-}"
if [[ -z "${artifact}" ]]; then
  artifact="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/mxr-v1-launch-proof.$(date -u +%Y%m%dT%H%M%SZ).jsonl"
fi
mkdir -p "$(dirname "${artifact}")"
: >"${artifact}"

emit() {
  local extra="${3-}"
  if [[ -z "${extra}" ]]; then
    extra='{}'
  fi
  python3 - "$artifact" "$1" "$2" "$extra" <<'PY'
import json, sys, time
path, step, status, raw_extra = sys.argv[1:5]
extra = json.loads(raw_extra or '{}')
row = {"ts": time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), "step": step, "status": status}
row.update(extra)
with open(path, 'a', encoding='utf-8') as f:
    f.write(json.dumps(row, sort_keys=True) + '\n')
print(json.dumps(row, sort_keys=True))
PY
}

run_mxr() {
  if [[ -n "${MXR_BIN:-}" ]]; then
    "${MXR_BIN}" "$@"
  elif [[ -x "${root}/target/debug/mxr" ]]; then
    "${root}/target/debug/mxr" "$@"
  elif [[ -x "${root}/target-cli/debug/mxr" ]]; then
    "${root}/target-cli/debug/mxr" "$@"
  else
    cargo run --quiet --bin mxr -- "$@"
  fi
}

tmp="$(mktemp -d)"
cleanup() {
  if [[ -n "${daemon_pid:-}" ]]; then
    kill "${daemon_pid}" 2>/dev/null || true
  fi
  rm -rf "${tmp}"
}
trap cleanup EXIT

export MXR_DATA_DIR="${tmp}/data"
export MXR_CONFIG_DIR="${tmp}/config"
export MXR_INSTANCE="mxr-v1-launch-proof-$$-$(date +%s%N)"
export EDITOR=true
mkdir -p "${MXR_DATA_DIR}" "${MXR_CONFIG_DIR}"
cat >"${MXR_CONFIG_DIR}/config.toml" <<'TOML'
[general]
default_account = "fake"

[bridge]
enabled = false

[agents.profiles.agent]
safety_policy = "draft-only"
allowed_accounts = ["fake"]
allow_send = false
allow_destructive = false

[agents.profiles.mcp]
safety_policy = "full"
allowed_accounts = ["fake"]
allow_send = true
allow_destructive = true

[accounts.fake]
name = "Fake Account"
email = "fake@example.com"

[accounts.fake.sync]
type = "fake"

[accounts.fake.send]
type = "fake"
TOML
emit configure_fake_account ok '{"provider":"fake","network":"none"}'

version="$(run_mxr --version | head -n1)"
emit binary_invocation ok "$(python3 -c 'import json,sys; print(json.dumps({"version": sys.argv[1]}))' "$version")"

status_json="$(run_mxr status --format json)"
daemon_pid="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("daemon_pid") or "")' <<<"${status_json}")"
if [[ -z "${daemon_pid}" ]]; then
  echo "daemon did not auto-start" >&2
  exit 1
fi
emit daemon_autostart ok "$(python3 -c 'import json,sys; print(json.dumps({"daemon_pid": int(sys.argv[1])}))' "$daemon_pid")"

run_mxr sync --wait --wait-timeout-secs 30 >/dev/null
emit sync ok '{"provider":"fake"}'

search_json="$(run_mxr search deployment --format json --limit 10)"
message_id="$(python3 -c 'import json,sys; v=json.load(sys.stdin); a=v if isinstance(v,list) else v.get("results",[]); print(a[0]["message_id"] if a else "")' <<<"${search_json}")"
if [[ -z "${message_id}" ]]; then
  echo "fake search returned no messages" >&2
  exit 1
fi
emit search ok "$(python3 -c 'import json,sys; print(json.dumps({"query":"deployment","first_message_id":sys.argv[1]}))' "$message_id")"

cat_json="$(run_mxr cat "${message_id}" --format json)"
thread_id="$(python3 -c 'import json,sys; v=json.load(sys.stdin); print(v.get("thread_id") or "")' <<<"${cat_json}")"
emit read_message ok "$(python3 -c 'import json,sys; print(json.dumps({"message_id":sys.argv[1],"thread_id":sys.argv[2]}))' "$message_id" "$thread_id")"

subject="mxr-v1-launch-proof-$(date +%s%N)"
compose_out="$(run_mxr compose --to launch-proof@example.invalid --subject "${subject}" --body "Launch proof body intentionally omitted from artifacts.")"
draft_id="$(awk '/^Draft saved: / {print $3}' <<<"${compose_out}")"
if [[ -z "${draft_id}" ]]; then
  echo "compose did not save a draft" >&2
  echo "${compose_out}" >&2
  exit 1
fi
emit draft_saved ok "$(python3 -c 'import json,sys; print(json.dumps({"draft_id":sys.argv[1],"subject":sys.argv[2]}))' "$draft_id" "$subject")"

send_preview="$(run_mxr send "${draft_id}" --dry-run --format json)"
emit send_preview ok "$(python3 -c 'import json,sys; v=json.load(sys.stdin); print(json.dumps({"dry_run": v.get("dry_run"), "draft_id": v.get("draft",{}).get("id") or v.get("draft_id")}))' <<<"${send_preview}")"

draft_json_file="${tmp}/agent-draft.json"
run_mxr drafts --format json >"${tmp}/drafts.json"
python3 - "$draft_id" "${tmp}/drafts.json" "${draft_json_file}" <<'PY'
import json, sys
want, src, dst = sys.argv[1:4]
v = json.load(open(src, encoding='utf-8'))
drafts = v if isinstance(v, list) else v.get('drafts', [])
draft = next((d for d in drafts if d.get('id') == want), None)
if draft is None:
    raise SystemExit('proof draft missing from drafts --format json')
with open(dst, 'w', encoding='utf-8') as f:
    json.dump(draft, f)
PY

agent_json="$(python3 - "$MXR_INSTANCE" "$message_id" "$draft_id" "$draft_json_file" <<'PY'
import json, os, socket, struct, sys, uuid

instance, message_id, draft_id, draft_path = sys.argv[1:5]
if sys.platform == 'darwin':
    sock_path = os.path.join(os.path.expanduser('~'), 'Library', 'Application Support', instance, 'mxr.sock')
else:
    sock_path = os.path.join(os.environ.get('XDG_RUNTIME_DIR') or '/tmp', instance, 'mxr.sock')

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(sock_path)
next_id = 1

def recv_exact(n):
    chunks = []
    remaining = n
    while remaining:
        chunk = sock.recv(remaining)
        if not chunk:
            raise RuntimeError('daemon closed IPC socket')
        chunks.append(chunk)
        remaining -= len(chunk)
    return b''.join(chunks)

def call(req):
    global next_id
    msg = {'id': next_id, 'source': 'agent', 'payload': {'type': 'Request', **req}}
    next_id += 1
    raw = json.dumps(msg, separators=(',', ':')).encode()
    sock.sendall(struct.pack('>I', len(raw)) + raw)
    while True:
        size = struct.unpack('>I', recv_exact(4))[0]
        resp = json.loads(recv_exact(size))
        if resp.get('id') == msg['id'] and resp.get('payload', {}).get('type') == 'Response':
            return resp

try:
    try:
        body = call({'cmd': 'GetBody', 'message_id': message_id})
    except Exception as e:
        raise RuntimeError('agent GetBody IPC failed') from e
    draft = json.load(open(draft_path, encoding='utf-8'))
    draft['id'] = str(uuid.uuid4())
    draft['body_markdown'] = 'Agent draft-only proof body omitted from artifacts.'
    draft['subject'] = draft.get('subject', '') + ' [agent-save]'
    try:
        saved = call({'cmd': 'SaveDraft', 'draft': draft})
    except Exception as e:
        raise RuntimeError('agent SaveDraft IPC failed with draft keys ' + json.dumps(sorted(draft.keys()))) from e
    try:
        blocked_send = call({'cmd': 'SendStoredDraft', 'draft_id': draft_id})
    except Exception as e:
        raise RuntimeError('agent SendStoredDraft IPC failed') from e
    try:
        blocked_destructive = call({'cmd': 'SetFlags', 'message_id': message_id, 'flags': 'READ'})
    except Exception as e:
        raise RuntimeError('agent SetFlags IPC failed') from e
finally:
    sock.close()

print(json.dumps({'get_body': body, 'save_draft': saved, 'blocked_send': blocked_send, 'blocked_destructive': blocked_destructive}))
PY
)"
python3 - "$agent_json" <<'PY'
import json, sys
v=json.loads(sys.argv[1])
for key in ['get_body', 'save_draft']:
    assert v[key]['payload']['status'] == 'Ok', (key, v[key])
for key in ['blocked_send', 'blocked_destructive']:
    assert v[key]['payload']['status'] == 'Error', (key, v[key])
assert 'agent profile' in v['blocked_send']['payload'].get('message', ''), v['blocked_send']
assert 'agent profile' in v['blocked_destructive']['payload'].get('message', ''), v['blocked_destructive']
PY
emit agent_ipc_policy ok "$(python3 -c 'import json,sys; v=json.loads(sys.argv[1]); print(json.dumps({"source":"agent","allowed_read":v["get_body"]["payload"]["status"]=="Ok","allowed_draft":v["save_draft"]["payload"]["status"]=="Ok","blocked_send":v["blocked_send"]["payload"]["status"]=="Error","blocked_destructive":v["blocked_destructive"]["payload"]["status"]=="Error","daemon_policy_gate": all("agent profile" in v[k]["payload"].get("message", "") for k in ["blocked_send", "blocked_destructive"])}))' "$agent_json")"

mcp_json="$(python3 - "$draft_id" "$message_id" <<'PY'
import json, os, subprocess, sys

draft_id, message_id = sys.argv[1:3]
cmd = [os.environ.get('MXR_BIN')] if os.environ.get('MXR_BIN') else None
if cmd:
    cmd += ['mcp', 'serve']
elif os.path.exists('target/debug/mxr'):
    cmd = ['target/debug/mxr', 'mcp', 'serve']
elif os.path.exists('target-cli/debug/mxr'):
    cmd = ['target-cli/debug/mxr', 'mcp', 'serve']
else:
    cmd = ['cargo', 'run', '--quiet', '--bin', 'mxr', '--', 'mcp', 'serve']
proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=os.environ.copy())
next_id = 1

def send(method, params=None, notify=False):
    global next_id
    msg = {'jsonrpc':'2.0','method':method}
    if params is not None:
        msg['params'] = params
    if not notify:
        msg['id'] = next_id
        next_id += 1
    proc.stdin.write(json.dumps(msg).encode() + b'\n')
    proc.stdin.flush()
    if notify:
        return None
    while True:
        line = proc.stdout.readline()
        if not line:
            err = proc.stderr.read().decode(errors='replace')
            raise RuntimeError('mcp server closed stdout: ' + err)
        response = json.loads(line)
        if response.get('id') == msg['id']:
            return response

try:
    initialize = send('initialize', {'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'mxr-v1-launch-proof','version':'1'}})
    send('notifications/initialized', notify=True)
    tools = send('tools/list')
    read_message = send('tools/call', {'name':'mxr_read_message','arguments':{'message_id':message_id,'include_body':False}})
    blocked = send('tools/call', {'name':'mxr_send_draft','arguments':{'draft_id':draft_id}})
    approved = send('tools/call', {'name':'mxr_send_draft','arguments':{'draft_id':draft_id,'confirm':True}})
finally:
    try:
        proc.stdin.close()
    except Exception:
        pass
    proc.terminate()
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()

print(json.dumps({'initialize': initialize, 'tools': tools, 'read_message': read_message, 'blocked': blocked, 'approved': approved}))
PY
)"
python3 - "$mcp_json" <<'PY'
import json, sys
v=json.loads(sys.argv[1])
tools=v['tools']['result']['tools']
names={t['name'] for t in tools}
assert 'mxr_send_draft' in names and 'mxr_read_message' in names, names
read_content=v['read_message']['result']['content'][0]['text']
read_payload=json.loads(read_content)
assert read_payload.get('message_id') or read_payload.get('envelope', {}).get('id'), read_content
content=v['blocked']['result']['content'][0]['text']
assert json.loads(content)['blocked'] is True, content
assert 'result' in v['approved'], v['approved']
PY
emit mcp_tools_and_gated_send ok "$(python3 -c 'import json,sys; v=json.loads(sys.argv[1]); names=[t["name"] for t in v["tools"]["result"]["tools"]]; print(json.dumps({"tool_count":len(names),"required_tools_present": all(n in names for n in ["mxr_send_draft","mxr_read_message"]),"read_message": "result" in v["read_message"],"blocked_without_confirm": True,"approved_send": "result" in v["approved"]}))' "$mcp_json")"

preview_json="$(run_mxr archive --dry-run --format json "${message_id}")"
emit mutation_preview ok "$(python3 -c 'import json,sys; v=json.load(sys.stdin); print(json.dumps({"dry_run": v.get("dry_run"), "action": v.get("action"), "message_count": len(v.get("message_ids", []))}))' <<<"${preview_json}")"

run_mxr archive --yes "${message_id}" >/dev/null
emit mutation_approved ok "$(python3 -c 'import json,sys; print(json.dumps({"action":"archive","message_id":sys.argv[1]}))' "$message_id")"

emit proof_complete ok "$(python3 -c 'import json,sys; print(json.dumps({"artifact":sys.argv[1]}))' "$artifact")"
echo "v1 launch proof artifact: ${artifact}"
