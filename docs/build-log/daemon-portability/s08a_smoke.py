#!/usr/bin/env python3
"""Exercise supervised stop/restart using an installed mxr binary and fake mail."""

from __future__ import annotations

import json
import os
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def fail(message: str) -> None:
    raise RuntimeError(message)


def fixture_environment(root: Path, instance: str, data_dir: Path, config_dir: Path, socket_path: Path) -> dict[str, str]:
    home_dir = root / "home"
    config_home = home_dir / ".config"
    data_home = home_dir / ".local" / "share"
    temp_dir = root / "tmp"
    for path in (home_dir, config_home, data_home, temp_dir, data_dir, config_dir):
        path.mkdir(parents=True, exist_ok=True)

    # The child gets no inherited provider credentials, mxr overrides, or user profile.
    return {
        "HOME": str(home_dir),
        "XDG_CONFIG_HOME": str(config_home),
        "XDG_DATA_HOME": str(data_home),
        "TMPDIR": str(temp_dir),
        "PATH": os.defpath,
        "MXR_INSTANCE": instance,
        "MXR_DATA_DIR": str(data_dir),
        "MXR_CONFIG_DIR": str(config_dir),
        "MXR_SOCKET_PATH": str(socket_path),
        "MXR_ACTIVITY": "off",
    }


def run_cli(binary: Path, env: dict[str, str], *args: str) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [str(binary), *args],
        env=env,
        text=True,
        capture_output=True,
        timeout=60,
        check=False,
    )
    if result.returncode != 0:
        fail(
            f"{binary.name} {' '.join(args)} exited {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def search_message_ids(binary: Path, env: dict[str, str]) -> list[str]:
    result = run_cli(binary, env, "search", "deployment", "--format", "json", "--limit", "50")
    try:
        payload = json.loads(result.stdout)
        rows = payload if isinstance(payload, list) else payload["results"]
        ids = [row["message_id"] for row in rows]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        fail(f"search returned unexpected JSON: {error}; stdout={result.stdout!r}")
    if not ids:
        fail("fake sync produced no searchable deployment messages")
    return ids


def read_exact(stream: socket.socket, size: int, deadline: float) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            fail("timed out while reading daemon IPC frame")
        stream.settimeout(remaining)
        part = stream.recv(size - len(chunks))
        if not part:
            fail("daemon closed IPC connection before completing an IPC frame")
        chunks.extend(part)
    return bytes(chunks)


def read_frame(stream: socket.socket, deadline: float) -> dict[str, object] | None:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        fail("timed out while waiting for daemon IPC frame or EOF")
    stream.settimeout(remaining)
    first = stream.recv(4)
    if not first:
        return None
    if len(first) < 4:
        first += read_exact(stream, 4 - len(first), deadline)
    length = struct.unpack(">I", first)[0]
    try:
        frame = json.loads(read_exact(stream, length, deadline))
    except json.JSONDecodeError as error:
        fail(f"daemon returned invalid IPC JSON: {error}")
    if not isinstance(frame, dict):
        fail(f"daemon returned a non-object IPC frame: {frame!r}")
    return frame


def ping_and_hold(socket_path: Path) -> socket.socket:
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(5)
    client.connect(str(socket_path))
    frame = json.dumps(
        {
            "id": 1,
            "source": "cli",
            "payload": {"type": "Request", "cmd": "Ping"},
        },
        separators=(",", ":"),
    ).encode()
    client.sendall(struct.pack(">I", len(frame)) + frame)
    deadline = time.monotonic() + 5
    try:
        while True:
            response = read_frame(client, deadline)
            if response is None:
                fail("daemon closed IPC connection before Ping response")
            payload = response.get("payload", {})
            if response.get("id") == 0 and payload.get("type") == "Event":
                continue
            data = payload.get("data", {})
            if (
                response.get("id") == 1
                and payload.get("type") == "Response"
                and payload.get("status") == "Ok"
                and data.get("kind") == "Pong"
            ):
                return client
            fail(f"daemon returned an unexpected Ping response: {response!r}")
    except Exception:
        client.close()
        raise


def wait_ready(binary: Path, env: dict[str, str], process: subprocess.Popen[bytes], socket_path: Path, pid_path: Path, log_path: Path) -> None:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        status = process.poll()
        if status is not None:
            fail(f"foreground daemon exited during startup ({status}); log:\n{log_path.read_text(errors='replace')}")
        if socket_path.exists() and pid_path.exists():
            result = subprocess.run(
                [str(binary), "status", "--format", "json"],
                env=env,
                text=True,
                capture_output=True,
                timeout=5,
                check=False,
            )
            if result.returncode == 0:
                return
        time.sleep(0.025)
    fail(f"daemon did not become ready; log:\n{log_path.read_text(errors='replace')}")


def stop_and_check(
    process: subprocess.Popen[bytes],
    sig: signal.Signals,
    socket_path: Path,
    pid_path: Path,
    client: socket.socket,
    log_path: Path,
    log_offset: int,
) -> None:
    os.kill(process.pid, sig)
    try:
        return_code = process.wait(timeout=20)
    except subprocess.TimeoutExpired:
        # Only the child process started by this smoke run is terminated here.
        process.kill()
        process.wait(timeout=5)
        fail(f"daemon did not exit after {sig.name}; its owned child was killed; log:\n{log_path.read_text(errors='replace')}")
    if return_code != 0:
        fail(f"daemon exited {return_code} after {sig.name}; log:\n{log_path.read_text(errors='replace')}")
    if socket_path.exists() or pid_path.exists():
        fail(f"{sig.name} left daemon-owned runtime files: socket={socket_path.exists()} pid={pid_path.exists()}")
    deadline = time.monotonic() + 10
    while True:
        frame = read_frame(client, deadline)
        if frame is None:
            break
        payload = frame.get("payload", {})
        if frame.get("id") == 0 and payload.get("type") == "Event":
            continue
        fail(f"daemon sent an unexpected IPC frame during shutdown: {frame!r}")
    log = log_path.read_bytes()[log_offset:].decode(errors="replace")
    marker = "SIGTERM received" if sig == signal.SIGTERM else "SIGINT received"
    if marker not in log or "daemon shutdown drain completed" not in log:
        fail(f"{sig.name} did not show the ordered shutdown in daemon log:\n{log}")


def start_daemon(binary: Path, env: dict[str, str], log_handle) -> subprocess.Popen[bytes]:
    return subprocess.Popen(
        [str(binary), "daemon", "--foreground", "--no-bridge"],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=log_handle,
        stderr=subprocess.STDOUT,
    )


def stop_owned_child(process: subprocess.Popen[bytes] | None) -> None:
    if process is None or process.poll() is not None:
        return
    process.send_signal(signal.SIGTERM)
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        # Cleanup is restricted to the Popen child created by this script.
        process.kill()
        process.wait(timeout=5)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {Path(sys.argv[0]).name} /path/to/mxr", file=sys.stderr)
        return 2
    if os.name != "posix":
        print("S08a smoke requires Unix domain sockets and POSIX signals", file=sys.stderr)
        return 2

    binary = Path(sys.argv[1]).expanduser().resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        print(f"mxr binary is missing or not executable: {binary}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="mxr-s08a-") as temp_name:
        root = Path(temp_name)
        data_dir = root / "data"
        config_dir = root / "config"
        socket_path = root / "mxr.sock"
        pid_path = data_dir / "daemon.pid"
        log_path = root / "daemon.log"
        instance = f"mxr-s08a-{os.getpid()}-{time.time_ns()}"
        env = fixture_environment(root, instance, data_dir, config_dir, socket_path)
        (config_dir / "config.toml").write_text(
            '[general]\ndefault_account = "fake"\n\n'
            '[bridge]\nenabled = false\n\n'
            '[search.semantic]\nenabled = false\nauto_download_models = false\n\n'
            '[llm]\nenabled = false\n\n'
            '[accounts.fake]\nname = "Fake Account"\nemail = "fake@example.com"\n\n'
            '[accounts.fake.sync]\ntype = "fake"\n\n'
            '[accounts.fake.send]\ntype = "fake"\n',
            encoding="utf-8",
        )

        process: subprocess.Popen[bytes] | None = None
        client: socket.socket | None = None
        with log_path.open("wb") as log_handle:
            try:
                process = start_daemon(binary, env, log_handle)
                first_log_offset = 0
                wait_ready(binary, env, process, socket_path, pid_path, log_path)
                run_cli(binary, env, "sync", "--wait", "--wait-timeout-secs", "30")
                before_ids = search_message_ids(binary, env)
                client = ping_and_hold(socket_path)
                stop_and_check(process, signal.SIGTERM, socket_path, pid_path, client, log_path, first_log_offset)
                client.close()
                client = None

                second_log_offset = log_path.stat().st_size
                process = start_daemon(binary, env, log_handle)
                wait_ready(binary, env, process, socket_path, pid_path, log_path)
                after_ids = search_message_ids(binary, env)
                if after_ids != before_ids:
                    fail(f"restart changed acknowledged message IDs: before={before_ids!r}, after={after_ids!r}")
                client = ping_and_hold(socket_path)
                stop_and_check(process, signal.SIGINT, socket_path, pid_path, client, log_path, second_log_offset)
                client.close()
                client = None
            except Exception as error:
                print(f"S08a smoke failed: {error}", file=sys.stderr)
                if client is not None:
                    client.close()
                stop_owned_child(process)
                return 1

        print(f"S08a stop/restart smoke passed for {binary}; retained {len(before_ids)} fake message IDs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
