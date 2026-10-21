# Launch-proof cleanup waits for daemon exit

Status: resolved in v0.6.117, source `51c747a5992b73a9c4729fdb4e6c325967cd7d4d`. Reproduced twice in [release v0.6.116](https://github.com/planetaryescape/mxr/actions/runs/39518033868):
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

The repair is commit `2957da9aa759dc297e50fea48a282d88272eaf70`.
`scripts/v1_launch_proof_cleanup.sh` sends SIGTERM only to a validated PID,
waits up to ten seconds for exit, and treats a zombie as stopped. Failed
process inspection and shutdown timeout retain the runtime. An original proof
failure keeps its exit status and diagnostic files. Follow-up commits
`283d3e897b9a52c747cdbbf1cbeb65e6ec39aa61` and
`4ed210f10e026f34cfdfef89e632c98999ab9b44` force the proof to use a socket
inside its own temporary runtime, check the selected path before accepting a
daemon PID, and give `mktemp` an explicit TMPDIR template. The direct IPC
client uses the same owned socket as the CLI and daemon.

Run the production helper regression:

```sh
bash scripts/v1_launch_proof_cleanup_test.sh
```

The delayed writer confirms the directory remains until shutdown finishes.
Other cases cover an already-reaped PID, no captured PID, timeout, invalid
PID, and preserving the original failure status. This test runs in the CI
Release Gate Scripts job. The helper path also participates in release scope
selection, with a scope regression test.

The frozen script passed all 13 fake-provider launch-proof rows using the
previously built candidate debug binary (reports `mxr 0.6.115`, SHA-256
`e6f5709bfdf0d46882580f9453919961aa7025e32c2a22179766249e1c7f89fd`).
Its captured daemon exited, socket disappeared, and temporary directory was
removed. Evidence:
`/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-blueprint-gaps-20261021/g01-launch-proof-cleanup-runtime/proof-cleanup-final.jsonl`.
No real mail, keychain or personal daemon was used. The fix shipped through
release-please in v0.6.117; the v0.6.116 tag and unpublished draft are preserved.

The final frozen proof at `4ed210f10` inherited a foreign sentinel socket.
All 13 proof rows passed; the sentinel accepted zero connections and stayed
alive through the proof. The owned daemon exited and its runtime/socket were
removed, then the sentinel was stopped and reaped. The receipt is
`g01-launch-proof-cleanup-runtime/socket-isolation-postconditions.json` under
the same run directory. This checks fixture isolation; it makes no claim
about an incident involving a personal daemon.

[Tagged release CI](https://github.com/planetaryescape/mxr/actions/runs/39526018048)
passed the launch-proof step on v0.6.117, including cleanup and the following
mailmerge verification. The Linux proof emitted all 13 rows as `ok`.
Receipt: `g01-release117-smoke-receipt.json` in the run directory above.
