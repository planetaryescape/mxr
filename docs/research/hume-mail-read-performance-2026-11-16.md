# Shared fixes pass behavior checks, but live acceptance remains open

The normal app is `https://mxr.justlivin.life` on Hume. Caddy forwards to
`https://hume-2.tail4d8fc3.ts.net:18741`. The installed daemon and gateway still
run v0.6.240, source `04bcfdff98ffa47951e6cb2469b4e62333f70c77`.
No release, push, live upgrade, service restart or demo-marker removal occurred.

The candidate source is `4ed2969d8cb2f13c3e1afe62c7a6be7a9f9751fc` on
`fix/mail-preview-evidence-performance`. The fetched base is `origin/main` at
`f7bf2bd35c70420a9453488bb90235a69416ed75`.
The [earlier audit](mail-view-performance-2026-10-26.md) records each shared fix
and its regressions. The frozen-mailbox check passes its one-second gateway
budget. Live signed-in checks still wait for an approved upgrade.

## The fixes apply to shared request paths

Workspace identity reads use the reader pool after initialization. Status
reads published semantic counters without waiting for background semantic work.
Native and browser authentication use fresh, atomic reader statements.
Completed revocations, ownership changes and paused recovery still deny access.
Browser presence updates no longer invalidate mail views.

Shared view preparation overlaps at most three account reads. The store uses
`UNION ALL` for disjoint inbox and filed selections. Reading resolves expiry
before extraction while retaining saved Later links, source history and finished
issues used for pace estimates.

The final preview change avoids loading classifier evidence across selected
threads. Preview text already loads bounded bodies and quote lookback separately.
Whole-thread metadata remains available for attachments and geometry.
This change affects the common Messages handler used by CLI, TUI and web.

At the candidate SHA, `remote.rs:51` authenticates `TouchSession` through the
reader path. `remote_lifecycle.rs:40` renews presence in memory. It performs no
database write. The session path needs no additional fix.

## The frozen mailbox stays on Hume

The fixture contains 112,415 messages and three stored accounts.
SQLite copied the live database through a pinned reader transaction in 216 seconds.
The fixture config uses `accounts = {}` to prevent provider creation.
The stored account records retain their IDs, backends and owned addresses.
Removing provider fields from configured accounts changes their derived IDs,
so that approach is invalid for this comparison.

The fixture disables LLM calls, semantic downloads, PDF prefetch and chimes.
Credentials, real attachments and raw replies remain on Hume.
Only aggregate timings, artifact identities and comparison verdicts leave it.

Each run restores the same database and lexical index through APFS copies.
The runner asserts the account and message counts and checks nonempty All and search replies.
All uses the frontend default of 500 rows per band.
The response contains five attention rows, 500 Recent rows and 500 Quiet rows.
Its matched count is 5,030. An attention count of five is not the total response size.

The harness runs the actual daemon and authenticated browser gateway on loopback.
It omits Caddy and TLS. It waits for `updates facts warmed`, then three seconds.
That marker does not prove that all startup work finished.
First requests lack a completed response-cache entry. Later requests can reuse valid views.
Other host workloads continue, and each sample records host load.

Hume has a sticky `demo-active` marker in the normal config directory.
It overrides explicit profile variables before ordinary CLI dispatch.
The fixture uses `sandbox-exec` to hide that one marker from test processes.
The real marker remains untouched. A live restart must first exit sticky demo mode
and check the normal data and socket paths.

## The first real-mail comparison preserves replies

Source `cfe55a100d1c95baa0e43869763236f5cdcce74d` contains all shared fixes
except the final preview change. All 11 normalized mail replies matched the
released baseline. Reading also retained its ranking buckets.
Normalization excludes generated time, age and live runway progress.
The comparison sorts existing equal-rank Reading ties by stable IDs.

Independent All requests took 1233/629/627 ms on v0.6.240 and
1260/62/60 ms on this candidate. Repeated reads improved, but the first request
still failed the one-second gateway budget.

The trace measured 1186 ms in All composition. Six preview-thread evidence
reads consumed about 563 ms in total. The final source change uses the existing
metadata-only thread reader for that helper. Summed query times do not establish
the critical path when operations overlap.

## Linux behavior checks and the demo pass

The candidate passed all 35 existing Messages tests, including quote fallback
and complete attachments. The required `cargo build -p mxr`, optimized build
and scoped Rust gate passed. The gate includes formatting, all-feature Clippy,
architecture checks and cargo-deny 0.20.2.

The unchanged shared code also passed 22 Reading tests, 111 other view tests,
561 store tests, 20 authority tests and 32 remote/cache tests.
Three real-mail reports and two store timing benchmarks remain ignored.

The demo runs the candidate at `http://localhost:42839/messages` with its
existing 50,000-message profile. Search took 104/41/39 ms, Messages took
549/15/14 ms, and shell took 178/118/119 ms after restart.
These are HTTP response times, not browser paint times or real-mail acceptance.

The copied Linux binary is `/tmp/mxr-demo-v0.6.241/mxr-preview-fixed`.
Its SHA256 is `88b10db6f78cc8a45effd9816aaa3a4ed7b1a8d8a19bbe2a29c3f596094899ed`.

## The final Hume check passes one second but fails 250 ms

The candidate macOS optimized build passed in 40 minutes and 36 seconds.
Its compiler cache exited during library compilation, and Cargo compiled directly.
The cause remains unproven. No global cache configuration changed.

The executable is `/tmp/mxr-real-mail-perf/mxr-preview-fixed` on Hume.
Its SHA256 is `a23eae44d3545b446c3d0ff010900277b33f7147a4d4f7f6a2fbb83de6fd0bb5`.
All 124 embedded asset hashes match the fresh production web build.
No heavy build owned by this task ran during measurement.
The measured one-minute host load ranged from 10.17 to 46.44.

Each cell contains first, second and third HTTP requests in milliseconds.
The runner sweeps routes in order. Later views can reuse facts from earlier views,
including the shared Now snapshot used by Rail.

| Route | Released v0.6.240 | Candidate 4ed2969d8 |
| --- | ---: | ---: |
| Messages, attention | 290/157/171 | 276/15/14 |
| Messages, All | 1165/655/658 | 595/63/62 |
| Messages, your turn | 161/189/187 | 17/15/15 |
| Messages, waiting on them | 206/201/249 | 174/15/16 |
| Now | 501/506/506 | 400/16/15 |
| Rail | 534/671/657 | 138/14/14 |
| To do | 66/75/66 | 54/53/55 |
| Updates, peek | 413/518/504 | 328/24/23 |
| Reading, peek | 505/317/357 | 244/199/19 |
| Archive | 488/88/86 | 117/68/70 |
| Lexical search | 67/26/29 | 29/23/23 |

Independent All restarts took 1238.33/634.72/622.59 ms on the release and
714.45/62.08/61.32 ms on the candidate. The normalized independent reply matched.
All 11 normalized sweep replies also matched, including Reading rank buckets.

The concurrent attention/rail/shell/search wave took 415/700/758/135 ms on the
candidate and 385/691/723/111 ms on the release.
These samples do not show a concurrent speedup.
Reading with `mark_visit=true` took 253/232/204 ms.
Updates with `mark_seen=true` took 378/347/330 ms.

The final verdict passes the 1000 ms gateway budget and fails 250 ms.

The largest candidate sample was 758.41 ms.

These results cover this snapshot
with providers disabled. They do not establish latency during live provider sync,
through Caddy, or at browser paint.
The [cold All issue](../issues/messages-cold-history-budget.md) and the earlier
synthetic cold-view failures remain open.

## Gateway reads complete while the fixture writer stays locked

An independent `BEGIN IMMEDIATE` transaction held the fixture writer for five seconds.
Authenticated gateway reads completed while that transaction remained open.

- Messages took 14.66 ms.
- Lexical search took 22.77 ms.
- Shell took 374.37 ms.

The transaction rolled back without data changes. Messages reused a ready view.
This checks authentication, presence, admission and cached delivery together.
It does not measure cold composition under continuous writes.

Run the private Hume harness outside the build queue:

```sh
/opt/homebrew/bin/python3.14 /tmp/mxr-real-mail-perf/run-preview.py
/opt/homebrew/bin/python3.14 /tmp/mxr-real-mail-perf/check-lock.py \
  --binary /tmp/mxr-real-mail-perf/mxr-preview-fixed \
  --label fixed-preview-lock --gateway --case writer-lock
```

Raw replies, the frozen database and preparation scripts stay in
`/tmp/mxr-real-mail-perf` on Hume. Aggregate evidence also exists on Linux in
`/tmp/mxr-mail-read-profile/hume-results`: `preview-acceptance.jsonl`,
`preview-verdict.json`, `preview-reply-comparison.jsonl`, `preview-lock.jsonl`
and `binaries.json`.
The interactive comparison is `/tmp/agent-htmlify/mxr-hume-latency.html` on Linux.

## Live installation still needs approval

The implementation and frozen-mailbox checks are complete on local branch
`fix/mail-preview-evidence-performance`. The Hume source branch is
`fix/hume-mail-read-performance`. Both contain the same candidate source.
No release, push or live upgrade occurred. Obsidian remains unchanged.

After approval, land the fixes and follow the release-please release chain.
Before restarting Hume, exit sticky demo mode and check the normal data and socket paths.
Upgrade the installed daemon and gateway, then check signed-in Messages,
All, search and the other measured routes through `https://mxr.justlivin.life`.
Measure first loads and repeat visits separately with normal sync enabled.
Keep the 250 ms issue open until independent cold reads consistently pass.

BK's outside-repository approval rule applies to installing the live app and
restarting Hume's services. Machine access alone does not approve that upgrade.
