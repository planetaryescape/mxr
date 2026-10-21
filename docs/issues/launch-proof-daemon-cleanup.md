# Launch-proof cleanup does not wait for daemon exit

Status: open. Reproduced twice in [release v0.6.116](https://github.com/planetaryescape/mxr/actions/runs/39518033868):
job `118954899126` at `2026-10-21T00:08:03Z` and retry job
`118962649238` at `2026-10-21T00:24:45Z`. Source:
`v0.6.116`, commit `748d2e9d6b5cba4374d1cd5fbd8ed035b4563d9f`.

All 13 JSONL proof rows, including `proof_complete`, reported `ok`. The step
then exited 1 with:

```text
rm: cannot remove '/tmp/tmp.9fwDVnwvAz/data/logs': Directory not empty
```

The second attempt reported the same error under `/tmp/tmp.UIXfk8ohCX`.
The release remained a draft. Mailmerge and the later Linux ownership and
Archive checks did not run in that attempt. Sanitized proof rows and the error
are retained in
`/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-blueprint-gaps-20261021/g01-release-smoke-attempt1.json`
and `g01-release-smoke-attempt2.json` in the same directory.

At that SHA, `scripts/v1_launch_proof.sh:42-49` sends SIGTERM to the detached
daemon PID, then immediately removes its temporary root. It does not wait for
the daemon to exit. The error is consistent with a writer modifying the logs
directory during removal; the captured evidence does not identify the writer
or prove a write after SIGTERM.

Wait for the captured daemon PID to exit within a bound before removing the
temporary root. On timeout, fail with the PID and isolated runtime path and
retain the directory for diagnosis. Existing bounded process-exit checks are
in `crates/daemon/tests/cli_journey.rs:1422-1445` and
`crates/daemon/tests/reset_cli.rs:341-356` at the same SHA.

Verify with an owned background-process fixture that delays exit after
SIGTERM and writes under `data/logs`. Assert that cleanup waits for exit before
removal, and cover the timeout path. Keep this test independent of provider
behavior and real mail. If a tagged release needs the repair, ship a new
release through release-please; preserve the existing tag and assets.
