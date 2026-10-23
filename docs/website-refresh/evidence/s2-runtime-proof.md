# S2 runtime proof: synthetic subscription search

Checked 2026-10-23 with the published macOS arm64 binary `mxr 0.6.156`. The isolated fake-provider profile synced 55 messages into each of two accounts. The local search query `newsletter` returned one result in each account, and `subscriptions --limit 200 --rank` returned four eligible sender buckets per account. The default unscoped subscription request returned only the configured default account; use `--account` once per account to show both.

## Provenance

- Repository: `origin/main`, `f39ae66578e3099e464967ab2f042e24b1ad8ec4`; worktree `HEAD` and merge base were the same SHA. Tree at that commit: `9ecf26304579bd14e0fc5e8cc63db8435a1f99df`.
- Fetch: `git fetch https://github.com/planetaryescape/mxr.git main:refs/remotes/origin/main` succeeded. SSH fetch had failed with a public-key error in the parent audit; this HTTPS fetch did not change the remote URL.
- Release: published, non-prerelease `v0.6.156`, on the same source commit. Downloaded `mxr-v0.6.156-macos-aarch64.tar.gz` from the GitHub release into this fixture. GitHub asset digest and local `shasum -a 256` both returned `8f428d8324bd34e342d14d66526459ac0290dc3bdb8f6caebf0ababf78b9b8c9`. The extracted binary reports `mxr 0.6.156 (Mixer)`. The installed `/opt/homebrew/bin/mxr` remains `0.6.155` and was not used for runtime commands.

Code claims below refer to `origin/main` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4`:

- `crates/config/src/resolve.rs#L28-L103` resolves config, data, token and secrets paths from explicit `MXR_*` overrides. `#L143-L170` does the same for bridge and daemon tokens.
- `crates/daemon/src/commands/demo.rs#L75-L119` checks the sticky marker under `dirs::config_dir()` before normal CLI dispatch; `crates/daemon/src/lib.rs#L75-L87` applies it to non-demo commands.
- `crates/provider-fake/src/lib.rs#L213-L224` creates canonical synthetic fixtures when `MXR_FAKE_DATASET` is unset. `crates/daemon/src/state.rs#L1188-L1205` binds configured fake sync accounts to that provider.
- `crates/daemon/src/cli/mod.rs#L1063-L1075` defines the default subscription limit of 200 and the account/rank options. `crates/daemon/src/commands/subscriptions.rs#L95-L129` sends the request and sorts the returned set when ranking.
- `crates/daemon/src/handler/diagnostics/mod.rs#L428-L441` applies the configured default account to unscoped subscription requests. `crates/daemon/src/commands/mod.rs#L156-L163` leaves an omitted optional account unset.
- `crates/store/src/message.rs#L1093-L1168` chooses one latest row per account and lowercased sender, excludes messages with no unsubscribe method and spam/trash, calculates `opened_count` from the READ flag at `#L1112-L1114` and `#L1155-L1156`, and applies `LIMIT` after selecting each bucket at `#L1150-L1153`.
- `crates/daemon/src/commands/demo.rs#L677-L680` explains that `last_synced_count` is only the latest landed page count, not the mailbox total.

## Isolation

The fixture root is `.cache/website-s2/`. `config/config.toml` defines only `personal` and `work`, both with `sync.type = "fake"` and `email = ...@example.invalid`. It disables the bridge, semantic search/model downloads and LLM. The runner `run-s2.sh` launches the downloaded binary with `env -i`; it does not set or repurpose `HOME`.

Before the first non-meta command, `.cache/website-s2/ownership.md` registered the fixture and runtime paths. The runner pins config, data, token, secrets, bridge-token, daemon-token, bridge-port and Unix socket paths below `.cache/website-s2/`. The daemon log and SQLite/search-index paths resolve below the pinned data directory. `MXR_ACTIVITY=off` disables activity writes. No real account config, database, token or session was read.

The exact macOS sticky marker `/Users/bhekanik/Library/Application Support/mxr/demo-active` was absent before and after the run. Every non-meta CLI invocation used `sandbox-exec` rules denying reads and writes to that exact path, closing the concurrent-marker redirection risk. A sandboxed read probe against the existing `/Users/bhekanik/.zshrc` returned `Operation not permitted`, confirming the deny rule is active. The fixture did not invoke `mxr demo`.

No network-backed provider was configured or called. The only external requests for this proof were GitHub release metadata and the binary download, plus the required HTTPS source fetch. No account connection, send, unsubscribe, build, test suite, deploy or site change was performed.

## Commands and observations

Release acquisition and integrity check:

```sh
gh release view v0.6.156 --repo planetaryescape/mxr --json tagName,isDraft,isPrerelease,assets
curl -fL https://github.com/planetaryescape/mxr/releases/download/v0.6.156/mxr-v0.6.156-macos-aarch64.tar.gz -o .cache/website-s2/release/mxr-v0.6.156-macos-aarch64.tar.gz
shasum -a 256 .cache/website-s2/release/mxr-v0.6.156-macos-aarch64.tar.gz
tar -xzf .cache/website-s2/release/mxr-v0.6.156-macos-aarch64.tar.gz -C .cache/website-s2/release
.cache/website-s2/release/mxr --version
```

The isolated runner applies the marker sandbox and path environment for each command. Runtime commands and captured outputs:

```sh
.cache/website-s2/run-s2.sh sync --wait --wait-timeout-secs 30 --format json
.cache/website-s2/run-s2.sh status --format json
.cache/website-s2/run-s2.sh count 'after:2000-01-01' --account personal --format json
.cache/website-s2/run-s2.sh count 'after:2000-01-01' --account work --format json
.cache/website-s2/run-s2.sh subscriptions --limit 200 --rank --format json
.cache/website-s2/run-s2.sh subscriptions --account personal --limit 200 --rank --format json
.cache/website-s2/run-s2.sh subscriptions --account work --limit 200 --rank --format json
.cache/website-s2/run-s2.sh search newsletter --account personal --format json --limit 200
.cache/website-s2/run-s2.sh search newsletter --account work --format json --limit 200
```

`sync.stdout.json` reported these accounts and IDs:

| Account key | Account ID | Sync result |
|---|---|---|
| `personal` | `2eabdffa-98d1-5441-9b15-e23b853084f2` | healthy; `55/55` stored in progress output |
| `work` | `3e41aaab-3d0e-51d7-8989-801642e361ac` | healthy; `55/55` stored in progress output |

The independent local count query returned `{"count":55,"query":"after:2000-01-01"}` for each account. Sync status reported `last_synced_count: 0` for personal and `55` for work; that field is a per-sync-page status value, not the total mailbox count. The local count query and sync progress are the corpus counts used here.

The unscoped subscription command returned 4 rows, all for `personal`, because the daemon resolves an omitted account to the configured default. Explicit account selection returned 4 rows for each account (8 account-scoped rows total). Each account had the same four fake sender buckets, with distinct account IDs and distinct UUID message IDs:

| Sender bucket | Personal latest message ID | Work latest message ID | `message_count` / `opened_count` |
|---|---|---|---|
| Hacker News (`digest@hn.algolia.com`) | `01a16943-e073-7350-a029-b4250c0aa3da` | `01a16943-e080-73e9-944d-a057ab2b2f61` | `1 / 0` each |
| This Week in Rust (`noreply@rust-lang.org`) | `01a16943-e073-7350-a029-b41274a53f5d` | `01a16943-e080-73e9-944d-a04450d3d0f9` | `1 / 0` each |
| The Changelog (`noreply@changelog.com`) | `01a16943-e073-7350-a029-b42fd8b8365b` | `01a16943-e080-73e9-944d-a061fbabc96e` | `1 / 0` each |
| Vic Security (`vic@work.com`) | `01a16943-e073-7350-a029-b43b0305fdda` | `01a16943-e080-73e9-944d-a06d88c002ee` | `1 / 0` each |

The `newsletter` search returned one unread The Changelog result per account. Its message ID matched that account's subscription row: personal `01a16943-e073-7350-a029-b42fd8b8365b`, work `01a16943-e080-73e9-944d-a061fbabc96e`. Both result JSON files record `read: false`.

The query found 55 local messages per account; `--limit 200` exceeds that fixture corpus, and four eligible sender buckets were returned per account. This is complete coverage of the eligible buckets in this 55-message synthetic fixture. It does not establish coverage of every newsletter or sender in a real mailbox. In this result, `opened_count` was 0 because the single message in each bucket was not marked READ. It represents current READ flags, not actual or distinct opens. Ranking sorts only the limited response returned by the store.

## Retained artifacts and checks

All fixture outputs are retained under `.cache/website-s2/`: the release archive and extracted release files, isolated config and runner, sync/status/count outputs, subscription JSON, search JSON, logs, SQLite database, search index and generated fake attachments. No cleanup or deletion was performed. The fixture occupied 327,544 KiB at closeout; release contents were 320,212 KiB and data was 7,272 KiB. The owned daemon used PID `34850` and Unix socket `.cache/website-s2/data/mxr.sock`; `lsof -nU -a -p 34850` showed the daemon's fixture socket, and `lsof -nP -iTCP -a -p 34850` showed no TCP listeners. It was stopped with `SIGTERM` after checking its PID, executable path and instance. PID `34850` exited and its socket and PID file were gone afterward. No queue job was started.

Checks run: release SHA-256 matched GitHub metadata; binary version matched `0.6.156`; `sh -n .cache/website-s2/run-s2.sh` passed; marker absent before/after and marker sandbox deny verified; sync and all count/subscription/search commands exited 0; owned PID and socket were stopped/closed. No Rust build/tests or site build were needed. Code-simplifier review of the fixture runner found no useful in-scope simplification. TypeScript review is N/A because no JS/TS changed. Documentation-refiner closeout updated only this evidence file; no Obsidian or production documentation was touched.
