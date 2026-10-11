#!/usr/bin/env python3
"""Verify durable send recovery using only an isolated fake-provider profile."""

import argparse
import json
import os
import shutil
import signal
import socket
import sqlite3
import struct
import subprocess
import time
import uuid
from pathlib import Path
from urllib.request import Request, urlopen

p = argparse.ArgumentParser()
p.add_argument("--binary", type=Path, required=True)
p.add_argument("--runtime", type=Path, required=True)
p.add_argument("--evidence", type=Path, required=True)
p.add_argument("--port", type=int, default=18393)
a = p.parse_args()
r = a.runtime.resolve()
r.mkdir(parents=True, exist_ok=False)
for name in ["home", "tmp", "config", "data", "spool"]:
    (r / name).mkdir()
shutil.copy2(a.binary, r / "mxr")
binary = str(r / "mxr")
sock = "/tmp/mxr-s03-" + uuid.uuid4().hex[:10] + ".sock"
(r / "config/bridge-token").write_text("synthetic-s03-token\n")
(r / "config/bridge-token").chmod(0o600)
(r / "config/config.toml").write_text(f'''[general]
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
token_path = "{r / "config/bridge-token"}"
[accounts.synthetic]
name = "Synthetic S03"
email = "synthetic@example.com"
is_default = true
[accounts.synthetic.sync]
type = "fake"
[accounts.synthetic.send]
type = "fake"
''')
env = {
    k: v
    for k, v in os.environ.items()
    if not k.startswith("MXR_")
    and k
    not in {"HOME", "TMPDIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"}
}
env.update(
    HOME=str(r / "home"),
    TMPDIR=str(r / "tmp") + "/",
    MXR_INSTANCE="s03-" + uuid.uuid4().hex[:8],
    MXR_CONFIG_DIR=str(r / "config"),
    MXR_DATA_DIR=str(r / "data"),
    MXR_SOCKET_PATH=sock,
    MXR_ACTIVITY="off",
    MXR_FAKE_DATASET="small",
    MXR_FAKE_SPOOL_DIR=str(r / "spool"),
)
records = []
daemon = None


def cli(*args, timeout=30, short=False):
    e = env.copy()
    if short:
        e["MXR_IPC_TIMEOUT_SECS"] = "1"
    v = subprocess.run(
        [binary, *args], env=e, text=True, capture_output=True, timeout=timeout
    )
    records.append(
        {
            "args": args,
            "returncode": v.returncode,
            "stdout": v.stdout,
            "stderr": v.stderr,
        }
    )
    return v


def start():
    global daemon
    with (r / "daemon.log").open("ab") as log:
        daemon = subprocess.Popen(
            [binary, "daemon", "--foreground", "--bridge-port", str(a.port)],
            env=env,
            stdout=log,
            stderr=log,
            start_new_session=True,
        )
    for _ in range(150):
        if daemon.poll() is not None:
            raise RuntimeError((r / "daemon.log").read_text())
        pid_file = r / "data/daemon.pid"
        # CLI status can auto-start a daemon; wait for this owned foreground
        # process to publish its PID before probing after a crash.
        if (
            Path(sock).exists()
            and pid_file.exists()
            and pid_file.read_text().strip() == str(daemon.pid)
        ):
            v = cli("status", "--format", "json")
            if v.returncode == 0:
                assert json.loads(v.stdout)["daemon_pid"] == daemon.pid
                return
        time.sleep(0.1)
    raise RuntimeError("Owned daemon did not start")


def stop():
    if daemon and daemon.poll() is None:
        daemon.send_signal(signal.SIGINT)
        daemon.wait(timeout=20)


def count():
    return len(list((r / "spool").glob("accepted-*.json")))


def draft(subject):
    v = cli(
        "compose",
        "--to",
        "recipient@example.com",
        "--subject",
        subject,
        "--body",
        "Synthetic reviewed body",
        "--format",
        "json",
    )
    assert v.returncode == 0, v.stderr
    draft_id = json.loads(v.stdout)["draft_id"]
    v = cli("send", draft_id, "--dry-run", "--format", "json")
    assert v.returncode == 0, v.stderr
    data = json.loads(v.stdout)
    return draft_id, data["draft"]["account_id"], data["send_identity"]


def outcome(account, operation):
    v = cli("send-outcome", operation, "--account", account, "--format", "jsonl")
    assert v.returncode == 0, v.stderr
    return json.loads(v.stdout)


def wait_outcome(account, operation, state):
    for _ in range(150):
        value = outcome(account, operation)
        if value and value["state"] == state:
            return value
        time.sleep(0.1)
    raise RuntimeError("Durable outcome did not reach " + state)


def ipc(request):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(30)
        connection.connect(sock)
        data = json.dumps(
            {"id": 999, "source": "tui", "payload": {"type": "Request", **request}}
        ).encode()
        connection.sendall(struct.pack(">I", len(data)) + data)

        def read(length):
            data = b""
            while len(data) < length:
                part = connection.recv(length - len(data))
                if not part:
                    raise RuntimeError("Owned daemon closed IPC")
                data += part
            return data

        response = json.loads(read(struct.unpack(">I", read(4))[0]))["payload"]
        records.append({"ipc_request": request, "ipc_response": response})
        return response


try:
    start()
    draft_id, account, identity = draft("Lost response")
    marker = r / "spool/send-pause-after-accept"
    marker.touch()
    v = cli(
        "send",
        draft_id,
        "--expected-revision",
        str(identity["expected_revision"]),
        "--format",
        "json",
        short=True,
    )
    assert v.returncode != 0 and identity["operation_id"] in v.stderr, (
        v.returncode,
        v.stderr,
    )
    assert count() == 1, count()
    marker.unlink()
    accepted = wait_outcome(account, identity["operation_id"], "accepted")
    immediate_accepted = accepted.copy()
    for _ in range(100):
        if accepted["local_message_id"]:
            break
        time.sleep(0.1)
        accepted = outcome(account, identity["operation_id"])
    assert accepted["local_message_id"], accepted
    stop()
    start()
    assert outcome(account, identity["operation_id"])["state"] == "accepted"
    assert count() == 1
    # Query via the actual authenticated HTTP bridge after the draft is deleted.
    request = Request(
        f"http://127.0.0.1:{a.port}/api/v1/mail/send-outcomes/{identity['operation_id']}?account_id={account}",
        headers={"Authorization": "Bearer synthetic-s03-token"},
    )
    http = json.load(urlopen(request))
    assert http["outcome"]["state"] == "accepted"
    uncertain_id, account2, uncertain_identity = draft(
        "Accepted but response uncertain"
    )
    (r / "spool/send-uncertain").touch()
    v = cli(
        "send",
        uncertain_id,
        "--expected-revision",
        str(uncertain_identity["expected_revision"]),
        "--format",
        "json",
    )
    assert v.returncode != 0 and "uncertain" in v.stderr
    assert count() == 2
    assert outcome(account2, uncertain_identity["operation_id"])["state"] == "unknown"
    stop()
    start()
    for extra in [[], ["--operation-id", "new-operation"]]:
        v = cli(
            "send",
            uncertain_id,
            "--expected-revision",
            str(uncertain_identity["expected_revision"]),
            *extra,
        )
        assert v.returncode != 0
    v = cli("drafts", "resume", uncertain_id)
    assert v.returncode != 0 and "uncertain" in v.stderr
    assert count() == 2
    crash_id, crash_account, crash_identity = draft("Crash after acceptance")
    (r / "spool/send-uncertain").unlink()
    marker.touch()
    v = cli(
        "send",
        crash_id,
        "--expected-revision",
        str(crash_identity["expected_revision"]),
        short=True,
    )
    assert v.returncode != 0 and count() == 3
    daemon.kill()  # Only this harness's isolated daemon, paused before receipt persistence.
    daemon.wait(timeout=10)
    daemon = None
    marker.unlink()
    start()
    crash_outcome = outcome(crash_account, crash_identity["operation_id"])
    assert crash_outcome["state"] == "unknown"
    v = cli(
        "send",
        crash_id,
        "--expected-revision",
        str(crash_identity["expected_revision"]),
        "--operation-id",
        "after-crash",
    )
    assert v.returncode != 0 and count() == 3
    retry_id, retry_account, retry_identity = draft("Unchanged rejected send")
    (r / "spool/send-reject").touch()
    v = cli(
        "send",
        retry_id,
        "--expected-revision",
        str(retry_identity["expected_revision"]),
    )
    assert v.returncode != 0 and count() == 3
    assert (
        outcome(retry_account, retry_identity["operation_id"])["state"]
        == "not_accepted"
    )
    (r / "spool/send-reject").unlink()
    retry_operation = retry_identity["operation_id"] + ":retry:reviewed"
    request = Request(
        f"http://127.0.0.1:{a.port}/api/v1/mail/drafts/{retry_id}/send-stored?expected_revision={retry_identity['expected_revision']}&operation_id={retry_operation}",
        method="POST",
        headers={"Authorization": "Bearer synthetic-s03-token"},
    )
    json.load(urlopen(request))
    retry_outcome = outcome(retry_account, retry_operation)
    assert retry_outcome["state"] == "accepted" and count() == 4
    reminder_id, reminder_account, reminder_identity = draft("Machine reminder output")
    v = cli(
        "send",
        reminder_id,
        "--expected-revision",
        str(reminder_identity["expected_revision"]),
        "--remind-after",
        "in 2h",
        "--format",
        "jsonl",
    )
    assert v.returncode == 0, v.stderr
    lines = [json.loads(line) for line in v.stdout.splitlines()]
    assert len(lines) == 1 and lines[0]["state"] == "accepted"
    assert "Reminder set" in v.stderr and count() == 5
    no_local_id, no_local_account, no_local_identity = draft(
        "Accepted without local indexing"
    )
    with sqlite3.connect(r / "data/mxr.db") as db:
        db.execute(
            "CREATE TRIGGER s03_fail_local BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'controlled local ingest failure'); END"
        )
    v = cli(
        "send",
        no_local_id,
        "--expected-revision",
        str(no_local_identity["expected_revision"]),
        "--remind-after",
        "in 2h",
        "--format",
        "jsonl",
    )
    assert v.returncode == 0 and "Send accepted; reminder setup failed" in v.stderr, (
        v.stderr
    )
    no_local_result = json.loads(v.stdout)
    assert (
        no_local_result["state"] == "accepted"
        and no_local_result["local_message_id"] is None
    )
    assert (
        outcome(no_local_account, no_local_identity["operation_id"])["state"]
        == "accepted"
        and count() == 6
    )
    with sqlite3.connect(r / "data/mxr.db") as db:
        db.execute("DROP TRIGGER s03_fail_local")
    corrected_id, corrected_account, corrected_identity = draft("TUI rejected review")
    reviewed = json.loads(
        cli("send", corrected_id, "--dry-run", "--format", "json").stdout
    )["draft"]
    (r / "spool/send-reject").touch()
    rejected = ipc(
        {
            "cmd": "SendDraft",
            "draft": reviewed,
            "send_identity": corrected_identity,
            "override_safety_token": None,
        }
    )
    assert "did not accept" in rejected.get("message", "") and count() == 6
    (r / "spool/send-reject").unlink()
    assert (
        outcome(corrected_account, corrected_identity["operation_id"])["state"]
        == "not_accepted"
    )
    reviewed["subject"] = "TUI corrected review"
    updated = ipc({"cmd": "UpdateDraft", "draft": reviewed})["data"]["draft"]
    assert updated["revision"] == 2 and updated["id"] == corrected_id
    corrected_retry = {
        "operation_id": corrected_id + ":2:retry:reviewed",
        "expected_revision": 2,
    }
    sent = ipc(
        {
            "cmd": "SendDraft",
            "draft": updated,
            "send_identity": corrected_retry,
            "override_safety_token": None,
        }
    )
    assert (
        sent["status"] == "Ok"
        and sent["data"]["kind"] in {"SendReceipt", "SendOutcome"}
        and count() == 7
    ), sent
    corrected_outcome = outcome(corrected_account, corrected_retry["operation_id"])
    assert corrected_outcome["state"] == "accepted"
    result = {
        "immediate_accepted_query": immediate_accepted,
        "accepted_after_response_loss": accepted,
        "accepted_after_restart": outcome(account, identity["operation_id"]),
        "uncertain_after_restart": outcome(
            account2, uncertain_identity["operation_id"]
        ),
        "controlled_acceptances": count(),
        "crash_after_acceptance": crash_outcome,
        "unchanged_reviewed_retry": retry_outcome,
        "reminder_jsonl": lines,
        "accepted_without_local_id": no_local_result,
        "corrected_tui_request_sequence": corrected_outcome,
        "http_query_after_cleanup": http,
        "actions": records,
    }
    a.evidence.parent.mkdir(parents=True, exist_ok=True)
    a.evidence.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "actions"}))
finally:
    stop()
