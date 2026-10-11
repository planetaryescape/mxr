#!/usr/bin/env python3
"""Exercise supervised ownership, dependency diagnostics, and persisted mail."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
from threading import Event, Lock, Thread
import time


class ModelState:
    def __init__(self):
        self.lock = Lock()
        self.request_count = 0
        self.fail_next = False
        self.block_next = False
        self.block_started = Event()
        self.release_block = Event()


model = ModelState()


class ModelHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        with model.lock:
            model.request_count += 1
            should_fail = model.fail_next
            model.fail_next = False
            should_block = model.block_next
            model.block_next = False
        if should_block:
            model.block_started.set()
            if not model.release_block.wait(timeout=20):
                self.send_error(504)
                return
        if should_fail:
            self.send_response(503)
            self.end_headers()
            self.wfile.write(b"fixture failure")
            return
        response = json.dumps(
            {
                "id": "fixture-completion",
                "object": "chat.completion",
                "created": 1,
                "model": "fixture-model",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": "Fixture summary."},
                        "finish_reason": "stop",
                    }
                ],
            }
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, _format, *_args):
        return


def request_count():
    with model.lock:
        return model.request_count


def find_summary_observation(snapshot, status):
    return next(
        (
            item
            for item in snapshot.get("observations", [])
            if item.get("feature") == "summarize" and item.get("status") == status
        ),
        None,
    )


binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/mxr").resolve())
daemon_logs = []

with tempfile.TemporaryDirectory(prefix="mxr-s08c-") as temporary:
    root = Path(temporary)
    isolated_binary = root / "mxr"
    shutil.copy2(binary, isolated_binary)
    home = root / "home"
    home.mkdir()
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    config = root / "config"
    config.mkdir()
    data = root / "data"
    socket = root / "mxr.sock"
    instance = f"s08c-{os.getpid()}"
    model_server = ThreadingHTTPServer(("127.0.0.1", 0), ModelHandler)
    model_thread = Thread(target=model_server.serve_forever, daemon=True)
    model_thread.start()
    model_url = f"http://127.0.0.1:{model_server.server_port}/v1"
    (config / "config.toml").write_text(
        '[general]\ndefault_account = "fake"\n\n'
        '[bridge]\nenabled = false\n\n'
        '[search.semantic]\nenabled = false\nauto_download_models = false\n\n'
        '[llm]\nenabled = true\n'
        f'base_url = "{model_url}"\nmodel = "fixture-model"\n'
        'api_key_env = ""\nrequest_timeout_secs = 15\n\n'
        '[llm.overrides.relationship_summary]\nenabled = false\n\n'
        '[llm.overrides.commitments]\nenabled = false\n\n'
        '[deliveries]\nenabled = false\n\n'
        '[accounts.fake]\nname = "Fake Account"\nemail = "fake@example.com"\n\n'
        '[accounts.fake.sync]\ntype = "fake"\n\n'
        '[accounts.fake.send]\ntype = "fake"\n'
        '[accounts.missing]\nname = "Missing Credential"\nemail = "missing@example.com"\n\n'
        '[accounts.missing.sync]\ntype = "imap"\nhost = "imap.invalid"\nport = 993\n'
        'username = "missing@example.com"\npassword_ref = "fixture-missing-secret"\n'
        'auth_required = true\nuse_tls = true\n',
        encoding="utf-8",
    )
    env = {
        **{
            key: value
            for key, value in os.environ.items()
            if not key.startswith("MXR_")
            and key not in {"HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"}
        },
        "HOME": str(home),
        "TMPDIR": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_RUNTIME_DIR": str(runtime),
        "MXR_INSTANCE": instance,
        "MXR_CONFIG_DIR": str(config),
        "MXR_DATA_DIR": str(data),
        "MXR_SOCKET_PATH": str(socket),
        "MXR_DAEMON_AUTOSTART": "off",
        "MXR_KEYCHAIN": "off",
        "MXR_ACTIVITY": "off",
    }
    env.pop("MXR_DAEMON_ADDR", None)
    env.pop("MXR_PROFILE", None)
    daemon = None
    draining_summary = None

    def cli(*args, check=True, timeout=45):
        result = subprocess.run(
            [str(isolated_binary), *args], env=env, text=True, capture_output=True, timeout=timeout
        )
        if check and result.returncode:
            raise RuntimeError(f"{' '.join(args)} failed:\n{result.stdout}\n{result.stderr}")
        return result

    def start():
        global daemon
        log = root / f"daemon-{len(daemon_logs) + 1}.log"
        daemon_logs.append(log)
        output = log.open("ab")
        daemon = subprocess.Popen(
            [str(isolated_binary), "daemon", "--foreground", "--no-bridge"],
            env=env,
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if daemon.poll() is not None:
                raise RuntimeError(f"supervised daemon exited with {daemon.returncode}")
            try:
                status = json.loads(cli("status", "--format", "json").stdout)
                if status.get("daemon_pid") == daemon.pid:
                    output.close()
                    return
            except (RuntimeError, json.JSONDecodeError):
                pass
            time.sleep(0.1)
        output.close()
        raise RuntimeError("supervised daemon did not become ready")

    def stop():
        global daemon
        if daemon is not None:
            if daemon.poll() is None:
                daemon.send_signal(signal.SIGTERM)
                daemon.wait(timeout=25)
            daemon = None

    def assert_status_does_not_probe(expected_status=None):
        before = request_count()
        llm = json.loads(cli("llm", "status", "--format", "json").stdout)
        status = json.loads(cli("status", "--format", "json").stdout)
        assert request_count() == before, "diagnostic reads probed the model endpoint"
        if expected_status is None:
            assert find_summary_observation(llm, "succeeded") is None
            assert find_summary_observation(llm, "failed") is None
        else:
            observation = find_summary_observation(llm, expected_status)
            assert observation is not None, llm
        return status

    try:
        # This process stands in for systemd: clients must adopt it, never replace it.
        start()
        first_pid = daemon.pid
        assert json.loads(cli("status", "--format", "json").stdout)["daemon_pid"] == first_pid
        assert request_count() == 0, "the daemon probed the optional model before a feature request"
        initial_status = assert_status_does_not_probe()
        assert initial_status["feature_health"]["summarize"]["status"] == "healthy"

        cli("sync", "--wait", "--wait-timeout-secs", "30")
        search = json.loads(cli("search", "deployment", "--format", "json", "--limit", "50").stdout)
        search = search if isinstance(search, list) else search["results"]
        assert search, "fake sync did not persist searchable mail"
        before_ids = [row["message_id"] for row in search]
        first_thread = search[0]["thread_id"]
        second_search = json.loads(
            cli("search", "invoice", "--format", "json", "--limit", "50").stdout
        )
        second_search = second_search if isinstance(second_search, list) else second_search["results"]
        second_thread = next(
            (row["thread_id"] for row in second_search if row["thread_id"] != first_thread),
            None,
        )
        assert second_thread is not None, "fake sync did not provide a second thread for the drain check"
        assert_status_does_not_probe()

        with model.lock:
            model.fail_next = True
        failed_summary = json.loads(cli("summarize", first_thread, "--format", "json").stdout)
        assert "error" in failed_summary, failed_summary
        failed_status = assert_status_does_not_probe("failed")
        assert failed_status["feature_health"]["summarize"]["status"] == "degraded", failed_status
        assert failed_status["health_class"] == "healthy", failed_status

        recovered_summary = json.loads(cli("summarize", first_thread, "--format", "json").stdout)
        assert recovered_summary.get("summary") == "Fixture summary.", recovered_summary
        recovered_status = assert_status_does_not_probe("succeeded")
        assert recovered_status["feature_health"]["summarize"]["status"] == "healthy", recovered_status
        assert recovered_status["health_class"] == "healthy", recovered_status
        after_observation_count = request_count()

        failed_sync = cli(
            "sync", "--account", "missing", "--wait", "--wait-timeout-secs", "30", check=False
        )
        assert failed_sync.returncode != 0, "sync unexpectedly succeeded without credentials"
        doctor = json.loads(cli("doctor", "--format", "json").stdout)
        findings = doctor.get("findings", [])
        missing = next(
            (finding for finding in findings if finding.get("message") == "Missing mail credential for missing"),
            None,
        )
        assert missing is not None, doctor
        assert any("secrets.toml" in step for step in missing.get("remediation", [])), missing
        assert "fixture-missing-secret" not in json.dumps(missing), missing

        # Hold a second feature request open so the daemon is certainly draining.
        with model.lock:
            model.block_next = True
        draining_summary = subprocess.Popen(
            [str(isolated_binary), "summarize", second_thread, "--format", "json"],
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        assert model.block_started.wait(timeout=30), "model fixture did not receive drain request"
        daemon.send_signal(signal.SIGTERM)
        deadline = time.monotonic() + 8
        unavailable = None
        while time.monotonic() < deadline:
            assert daemon.poll() is None, "daemon exited before the held request was released"
            candidate = cli("status", "--format", "json", check=False)
            if candidate.returncode and "service manager" in candidate.stderr.lower():
                unavailable = candidate
                break
            time.sleep(0.1)
        assert unavailable is not None, "draining daemon did not report supervised unavailability"
        assert socket.exists(), "daemon removed its socket before the held request drained"
        assert (data / "daemon.pid").read_text().strip() == str(first_pid)
        assert request_count() == after_observation_count + 1
        model.release_block.set()
        output, error = draining_summary.communicate(timeout=20)
        assert draining_summary.returncode == 0, f"held summary failed:\n{output}\n{error}"
        assert json.loads(output).get("summary") == "Fixture summary."
        daemon.wait(timeout=25)
        daemon = None

        absent = cli("status", "--format", "json", check=False)
        assert absent.returncode != 0, absent.stdout
        assert "service manager" in absent.stderr.lower(), absent.stderr
        assert not socket.exists(), "client created a replacement while supervised daemon was down"
        assert not (data / "daemon.pid").exists(), "client left a replacement daemon pid file"

        start()
        assert daemon.pid != first_pid
        after = json.loads(cli("search", "deployment", "--format", "json", "--limit", "50").stdout)
        after = after if isinstance(after, list) else after["results"]
        assert [row["message_id"] for row in after] == before_ids
        print(
            json.dumps(
                {
                    "supervised_owner_pid": daemon.pid,
                    "persisted_search_results": len(after),
                    "model_requests": request_count(),
                }
            )
        )
    except Exception:
        for log in daemon_logs:
            if log.exists():
                print(f"--- {log.name} ---", file=sys.stderr)
                print(log.read_text(errors="replace")[-12000:], file=sys.stderr)
        raise
    finally:
        model.release_block.set()
        if draining_summary is not None and draining_summary.poll() is None:
            draining_summary.communicate(timeout=20)
        stop()
        model_server.shutdown()
        model_server.server_close()
        model_thread.join(timeout=5)
