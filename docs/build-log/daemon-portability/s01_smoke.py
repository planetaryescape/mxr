#!/usr/bin/env python3
"""Exercise a built/released mxr against an isolated real daemon."""
import argparse
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import tempfile
import time


def stop(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGINT)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def invoke(binary, env, args):
    process = subprocess.Popen(
        [str(binary), *args], env=env, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, start_new_session=True,
    )
    try:
        out, err = process.communicate(timeout=15)
        return process.returncode, out, err
    finally:
        stop(process)


def stop_autostarted(binary, instance):
    # A failed isolation assertion can still leave the CLI's detached child.
    commands = subprocess.check_output(["ps", "-ax", "-o", "pid=,command="], text=True)
    for line in commands.splitlines():
        pid, _, command = line.strip().partition(" ")
        if command.strip() == f"{binary} daemon --instance {instance}":
            os.kill(int(pid), signal.SIGINT)


def run(binary):
    with tempfile.TemporaryDirectory(prefix="mxr-s01-", dir="/tmp") as temporary:
        root = Path(temporary)
        base_env = {k: v for k, v in os.environ.items() if not k.startswith("MXR_")}
        base_env.update(HOME=str(root / "home"), XDG_CONFIG_HOME=str(root / "xdg"))
        profiles = {}
        for name in ("client", "target"):
            profile = root / name
            (profile / "config").mkdir(parents=True)
            (profile / "config/config.toml").write_text(
                "[bridge]\nenabled = false\n[search.semantic]\nenabled = false\n"
                "auto_download_models = false\n[llm]\nenabled = false\n"
            )
            profiles[name] = dict(
                base_env, MXR_INSTANCE=f"mxr-s01-{name}-{os.getpid()}",
                MXR_CONFIG_DIR=str(profile / "config"),
                MXR_DATA_DIR=str(profile / "data"),
                MXR_SOCKET_PATH=str(profile / "socket"), MXR_ACTIVITY="off",
            )
        target = profiles["target"]
        client = profiles["client"]
        local_data = Path(client["MXR_DATA_DIR"])
        (local_data / "search_index").mkdir(parents=True)
        marker = local_data / "search_index/keep"
        marker.write_text("client-only sentinel")
        with (root / "daemon.log").open("w") as log:
            daemon = subprocess.Popen(
                [str(binary), "daemon", "--foreground", "--no-bridge"],
                env=target, stdout=log, stderr=log, start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 20
                while not Path(target["MXR_SOCKET_PATH"]).exists():
                    if daemon.poll() is not None or time.monotonic() >= deadline:
                        raise AssertionError((root / "daemon.log").read_text())
                    time.sleep(0.1)
                bridge = ["env", *[f"{key}={value}" for key, value in target.items()
                                   if key.startswith("MXR_")],
                          str(binary), "daemon", "dial-stdio"]
                wrapper = root / "bridge"
                wrapper.write_text("#!/bin/sh\nexec " + shlex.join(bridge) + "\n")
                wrapper.chmod(0o700)
                client["MXR_DAEMON_ADDR"] = "cmd://" + str(wrapper)
                code, out, err = invoke(binary, client, ["status", "--format", "json"])
                assert code == 0, err
                status = json.loads(out)
                assert status["daemon_pid"] == daemon.pid, status
                assert status["daemon_target"]["kind"] == "cmd", status
                assert status["client_local"]["data_dir"] == str(local_data), status
                assert "data_dir" not in status, status
                results = {"status_target_pid": True}
                for option in ("--check", "--store-stats", "--index-stats"):
                    code, out, err = invoke(binary, client, ["doctor", option, "--format", "jsonl"])
                    assert len(out.splitlines()) == 1, (out, err)
                    report = json.loads(out)
                    assert report["daemon_target"]["kind"] == "cmd", report
                    assert report["client_local"]["data_dir"] == str(local_data), report
                    field = "path" if option == "--index-stats" else "database_path"
                    assert report["daemon"][field].startswith(target["MXR_DATA_DIR"]), report
                    if option == "--check":
                        assert (code == 0) == report["daemon"]["healthy"], (report, err)
                    else:
                        assert code == 0, err
                    results[option] = True
                code, _, err = invoke(binary, client, ["doctor", "--reindex", "--check"])
                assert code != 0 and "local default" in err, err
                client["MXR_DAEMON_ADDR"] = f"unix://{root}/missing.sock"
                for args in (["status"], ["doctor", "--check"], ["doctor", "--store-stats"]):
                    code, _, err = invoke(binary, client, args)
                    assert code != 0 and "connect" in err, (args, code, err)
                assert marker.read_text() == "client-only sentinel"
                assert not (local_data / "mxr.db").exists()
                assert not Path(client["MXR_SOCKET_PATH"]).exists()
                results.update(remote_repair_rejected=True, unreachable_target_isolated=True)
                print(json.dumps(results, sort_keys=True))
            finally:
                stop(daemon)
                stop_autostarted(binary, client["MXR_INSTANCE"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    run(parser.parse_args().binary.resolve())
