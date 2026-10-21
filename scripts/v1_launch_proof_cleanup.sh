#!/usr/bin/env bash

v1_launch_proof_process_state() {
  python3 - "$1" <<'PY'
import os
import subprocess
import sys

pid = int(sys.argv[1])

try:
    os.kill(pid, 0)
except ProcessLookupError:
    sys.exit(0)
except PermissionError:
    sys.exit(2)

result = subprocess.run(
    ["ps", "-o", "stat=", "-p", str(pid)],
    capture_output=True,
    check=False,
)
state = result.stdout.decode("utf-8", errors="replace").strip()
if result.returncode != 0 or not state:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        sys.exit(0)
    except PermissionError:
        sys.exit(2)
    sys.exit(2)

sys.exit(0 if state.startswith("Z") else 1)
PY
}

v1_launch_proof_cleanup() {
  local original_status="$1"
  local daemon_pid="$2"
  local tmp="$3"
  local timeout_seconds="${4:-10}"
  local cleanup_status=0
  local deadline
  local process_state=1

  if [[ ! "$timeout_seconds" =~ ^[0-9]+$ ]] || (( timeout_seconds < 1 )); then
    printf 'launch proof cleanup: invalid shutdown timeout %q; runtime retained at %s\n' \
      "$timeout_seconds" "$tmp" >&2
    cleanup_status=1
  fi

  if (( cleanup_status == 0 )) && [[ -n "$daemon_pid" ]]; then
    if [[ ! "$daemon_pid" =~ ^[0-9]+$ ]] || (( daemon_pid <= 1 )); then
      printf 'launch proof cleanup: refusing invalid daemon PID %q; runtime retained at %s\n' \
        "$daemon_pid" "$tmp" >&2
      cleanup_status=1
    else
      if ! kill "$daemon_pid" 2>/dev/null; then
        if v1_launch_proof_process_state "$daemon_pid"; then
          process_state=0
        else
          process_state=$?
        fi
        if (( process_state != 0 )); then
          printf 'launch proof cleanup: could not signal daemon PID %s; runtime retained at %s\n' \
            "$daemon_pid" "$tmp" >&2
          cleanup_status=1
        fi
      fi

      if (( cleanup_status == 0 )); then
        deadline=$((SECONDS + timeout_seconds))
        while (( SECONDS < deadline )); do
          if v1_launch_proof_process_state "$daemon_pid"; then
            process_state=0
          else
            process_state=$?
          fi
          if (( process_state == 0 )); then
            break
          fi
          if (( process_state != 1 )); then
            printf 'launch proof cleanup: could not verify daemon PID %s; runtime retained at %s\n' \
              "$daemon_pid" "$tmp" >&2
            cleanup_status=1
            break
          fi
          sleep 0.1
        done

        if (( cleanup_status == 0 && process_state != 0 )); then
          printf 'launch proof cleanup: daemon PID %s did not exit within %s seconds; runtime retained at %s\n' \
            "$daemon_pid" "$timeout_seconds" "$tmp" >&2
          cleanup_status=1
        fi
      fi
    fi
  fi

  if (( cleanup_status == 0 && original_status == 0 )) && ! rm -rf "$tmp"; then
    printf 'launch proof cleanup: could not remove runtime at %s\n' "$tmp" >&2
    cleanup_status=1
  fi

  if (( original_status != 0 )); then
    if (( cleanup_status == 0 )); then
      printf 'launch proof failed with exit status %s; runtime retained at %s for diagnostics\n' \
        "$original_status" "$tmp" >&2
    fi
    return "$original_status"
  fi
  return "$cleanup_status"
}
