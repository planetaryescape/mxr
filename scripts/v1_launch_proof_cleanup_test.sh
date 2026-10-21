#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${root}/scripts/v1_launch_proof_cleanup.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/mxr-launch-proof-cleanup.XXXXXX")"

cleanup_test_root() {
  local pid
  for pid in "${delayed_pid:-}" "${stuck_pid:-}"; do
    [[ -n "$pid" ]] || continue
    kill -KILL "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  rm -rf "$test_root"
}

trap cleanup_test_root EXIT

runtime="${test_root}/delayed-runtime"
logs="${runtime}/data/logs"
marker="${test_root}/delayed-daemon-result"
mkdir -p "$logs"

python3 - "$logs" "$marker" <<'PY' &
import pathlib
import signal
import sys
import time

logs = pathlib.Path(sys.argv[1])
marker = pathlib.Path(sys.argv[2])

def stop(_signal, _frame):
    time.sleep(0.25)
    if logs.is_dir():
        (logs / "late.log").write_text("shutdown finished\n", encoding="utf-8")
        marker.write_text("runtime-present\n", encoding="utf-8")
    else:
        marker.write_text("runtime-missing\n", encoding="utf-8")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, stop)
marker.write_text("ready\n", encoding="utf-8")
while True:
    time.sleep(1)
PY
delayed_pid=$!

for _ in {1..100}; do
  [[ "$(cat "$marker" 2>/dev/null || true)" == ready ]] && break
  sleep 0.01
done
[[ "$(cat "$marker")" == ready ]] || {
  echo "delayed daemon fixture did not become ready" >&2
  exit 1
}

if ! v1_launch_proof_cleanup 0 "$delayed_pid" "$runtime" 5; then
  echo "cleanup failed for a daemon that exited after SIGTERM" >&2
  exit 1
fi
wait "$delayed_pid"
already_exited_pid="$delayed_pid"
delayed_pid=""
[[ "$(cat "$marker")" == runtime-present ]] || {
  echo "runtime was removed before the daemon finished its delayed log write" >&2
  exit 1
}
[[ ! -e "$runtime" ]] || {
  echo "runtime remained after daemon exit" >&2
  exit 1
}

runtime="${test_root}/already-exited-runtime"
mkdir -p "$runtime/data/logs"
printf 'preserve failed-run evidence\n' >"$runtime/data/logs/mxr.log"
if v1_launch_proof_cleanup 23 "$already_exited_pid" "$runtime" 1 2>"${test_root}/failed-run.stderr"; then
  echo "cleanup unexpectedly returned success for original status 23" >&2
  exit 1
else
  cleanup_status=$?
fi
[[ "$cleanup_status" -eq 23 ]] || {
  echo "cleanup changed original exit status 23 to ${cleanup_status}" >&2
  exit 1
}
[[ -f "$runtime/data/logs/mxr.log" ]] || {
  echo "diagnostic runtime was removed after a failed proof" >&2
  exit 1
}
grep -Fq "runtime retained at ${runtime} for diagnostics" "${test_root}/failed-run.stderr"
rm -rf "$runtime"

runtime="${test_root}/no-daemon-runtime"
mkdir -p "$runtime/data/logs"
if ! v1_launch_proof_cleanup 0 "" "$runtime" 1; then
  echo "cleanup failed when auto-start had not produced a daemon PID" >&2
  exit 1
fi
[[ ! -e "$runtime" ]] || {
  echo "runtime remained when there was no daemon PID" >&2
  exit 1
}

runtime="${test_root}/stuck-runtime"
mkdir -p "$runtime/data/logs"
python3 - "$test_root/stuck-ready" <<'PY' &
import pathlib
import signal
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
pathlib.Path(sys.argv[1]).write_text("ready\n", encoding="utf-8")
while True:
    time.sleep(1)
PY
stuck_pid=$!

for _ in {1..100}; do
  [[ -e "${test_root}/stuck-ready" ]] && break
  sleep 0.01
done
[[ -e "${test_root}/stuck-ready" ]] || {
  echo "stuck daemon fixture did not become ready" >&2
  exit 1
}

if v1_launch_proof_cleanup 23 "$stuck_pid" "$runtime" 1 2>"${test_root}/timeout.stderr"; then
  echo "cleanup unexpectedly succeeded while the daemon ignored SIGTERM" >&2
  exit 1
else
  cleanup_status=$?
fi
[[ "$cleanup_status" -eq 23 ]] || {
  echo "cleanup changed original exit status 23 to ${cleanup_status}" >&2
  exit 1
}
grep -Fq "daemon PID ${stuck_pid} did not exit within 1 seconds" "${test_root}/timeout.stderr"
[[ -d "$runtime/data/logs" ]] || {
  echo "runtime was removed after shutdown timed out" >&2
  exit 1
}
kill -KILL "$stuck_pid" 2>/dev/null || true
wait "$stuck_pid" 2>/dev/null || true
stuck_pid=""

runtime="${test_root}/invalid-pid-runtime"
mkdir -p "$runtime/data/logs"
if v1_launch_proof_cleanup 0 0 "$runtime" 1 2>"${test_root}/invalid-pid.stderr"; then
  echo "cleanup unexpectedly accepted PID 0" >&2
  exit 1
fi
grep -Fq "refusing invalid daemon PID" "${test_root}/invalid-pid.stderr"
[[ -d "$runtime/data/logs" ]] || {
  echo "runtime was removed for an invalid daemon PID" >&2
  exit 1
}

echo "v1_launch_proof_cleanup_test: ok"
