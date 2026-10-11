# Supervised stops drain the daemon and preserve acknowledged mail

S08a is released in [v0.6.64](https://github.com/planetaryescape/mxr/releases/tag/v0.6.64), source `6ed0554c24df5bfbb99aaaebec5fdfab38254f50`. [PR338](https://github.com/planetaryescape/mxr/pull/338) integrated the shutdown work; [release run 38128790755](https://github.com/planetaryescape/mxr/actions/runs/38128790755) completed successfully in 24m20s. The published Linux and macOS archives matched their checksums. The installer and Homebrew installed identical `mxr 0.6.64` binaries, SHA256 `289234a8eadf2dd78637b055b423debede8ba699acc7b8c0eb68ee149e9d4dd1`.

The Unix foreground daemon handles SIGINT and SIGTERM through the existing orderly shutdown path. Service shutdown requests share a deadline; unfinished registered async tasks are cancelled when it expires. Signal registration failure also cleans up the listeners and runtime it created. The implementation is in `crates/daemon/src/server.rs` and `crates/daemon/src/state.rs` at the release source above.

The installed-client smoke passed through both installation paths: 5.639 seconds for install.sh and 5.584 seconds for Homebrew. Each isolated fake profile retained four acknowledged message IDs across active SIGTERM and idle SIGINT shutdown/restart. Active SIGTERM allowed the named relationship analytics cancellation; the idle restart had no shutdown warnings. The [installed proof](evidence/s08a-installed-v0.6.64.json) records release, archive, binary and smoke hashes. No real mail or personal daemon was used. The [Mac gate summary](evidence/s08a-validation-4921-summary.md) records the earlier candidate run and binary identity.

| Phase | Route or model | Elapsed | Evidence |
| --- | --- | ---: | --- |
| Implementation | GPT-6-Luna/high | — | no escalation |
| Production review | Claude Opus 5.5/max | 16m52s | final review |
| Local gate, job 4921 | focused tests, Clippy and build | 9m30.267s | separate smoke took 5.744s; [summary](evidence/s08a-validation-4921-summary.md) |
| Exact-head CI | [run 38095310846](https://github.com/planetaryescape/mxr/actions/runs/38095310846) | 5m22s | passed |
| Release workflow | v0.6.64 | 24m20s | macOS native build 19m17s |
| Installed stop/restart smokes | installer / Homebrew | 5.639s / 5.584s | both passed |

The released implementation proves orderly stop and restart of acknowledged persisted mail. It does not promise replay of interrupted sends, mailbox actions or relationship analytics. S03/S04 own durable effect outcomes; [supervision follow-ups](../../issues/supervised-shutdown-followups.md) track analytics recovery, supervisor deadlines, owned-child escalation and supervised-client behavior. Non-Unix runtime behavior remains untested. Reproduce the release journey with `python3 docs/build-log/daemon-portability/s08a_smoke.py /absolute/path/to/mxr`. S02 subsequently passed installed CLI and embedded-browser acceptance in [v0.6.65](s02-draft-recovery.md). S03 owns durable send outcomes.
