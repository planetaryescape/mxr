# Remote web and desktop workflows await final acceptance

Continue `fix/remote-web-desktop-parity` in `/Users/bhekanik/code/planetaryescape/.worktrees/mxr-pool-3`. BK asked to fix all hosted web and desktop gaps. The completion check is two isolated paired clients on one synthetic workspace: browser login without URL credentials, read/search, preview and triage, upload/draft/send, restart/reconnect and outcome recovery, then revocation of one parent while the second continues. The desktop must use the same saved connection and cookie exchange. Hosting must preview owned service/proxy changes and preserve explicit stopped intent.

The candidate adds independently approved triage/AI grants, parent-bound browser child sessions and assistant tools/history, bounded portable inline images, desktop launch packets, leased presence, bounded drain, supervised hosting and the inherited selection/pairing/cleanup recovery fixes. [Runtime notes](remote-hosting-runtime.md) explain the host boundary; [the issue](../issues/remote-hosting-and-client-lifecycle.md) retains release and live acceptance gates.

## The source is committed and final checks remain

The main implementation is committed as `fdcecd6df117b17ebe209d664f0274113f0f4d68`; `d59dbc22c` removes the legacy daemon-stop guard and updates help snapshots. Both are on `fix/remote-web-desktop-parity`, based on `f4ab9360a275f7917177c4fd5af7c91c557ce495`. Documentation and generated CLI help are prepared for the follow-up commit. No PR or release exists yet. The retained stash is an older backup; the commit owns the later fixes.

Retained verification receipts:

| Check | Retained evidence | Outcome |
| --- | --- | --- |
| Scoped daemon/store/TUI regressions | `/tmp/mxr-pool3-final-scoped-rust-tests-20261106-r2.log` | 45 + 18 + 13 passed |
| Web TypeScript | `/tmp/mxr-pool3-final-web-typecheck-20261106.log` | Passed |
| LLM policy/cache and route panel Vitest | `/tmp/mxr-pool3-llm-web-tests-20261106.log` | 6 passed |
| Embedded SPA build | `/tmp/mxr-pool3-final-spa-build-20261106.log` | Passed |
| OpenAPI generation | `/tmp/mxr-pool3-api-generation-20261106.log` | Passed; generated policy field checked |
| CLI help integration | `/tmp/mxr-pool3-cli-help-20261106.log` | First run found a legacy argv guard rejecting daemon stop. Guard removed; rerun passed all six tests (`/tmp/mxr-pool3-cli-help-20261106-r2.log`). |

The immutable `dec837…` binary predates the LLM-status fix. Runs g/h remain incomplete acceptance receipts: h recorded 55 product checks but failed its fixture EOF handshake. The fixture now closes stdin before waiting and requires successful, redacted LLM status. Build and copy a new binary before the final default browser-only run. Native HTTPS CLI pairing and the actual desktop WebView remain unverified with the private fixture CA; no OS trust changes were made.

Exact next action: finish the final binary build (session 13207), then run default browser acceptance, scoped Rust hygiene and final web lint through the shared admission gate. Build the final web-enabled binary and rerun the default browser fixture. Reconcile docs, fetch/rebase fresh main, check IPC numbering, and open the draft PR. Second-opinion review remains waived; self-review and security regression checks still apply.

Pool-3's sole launcher is root, using `/tmp/mxr-heavy-admit.py` as a logging adapter around the tested shared `tools/resume_heavy_gate.py`. The exact host flock is `/Users/bhekanik/code/planetaryescape/.orchestrate/host-heavy-admission.lock`; hold it across global Running + Queued < 3 and enqueue visibility, release before waiting. One command per owner, maximum five workers. The old R3 reservation is cleared.

The one-time quality #120 handoff is satisfied: BK confirmed actual admission at 2026-11-06T02:02:03.392114Z, label `mxr-backlog-pass2:21a51b7f8f9146199dc6d8d5a9c3e009`. Normal admission resumed immediately; its later performance/root gate has no reservation. All existing jobs stay intact; no sentinel, queue configuration change or cancellation. Root and all three pool-3 workers received this handoff. The other MXR root's existing backlog ledger records that it already relayed the same instruction to all four workers, including active pool-5.

## Failed browser receipts explain the remaining run

The real HTTPS fixture exposed two production blockers that were fixed before the current commit: `Referrer-Policy: no-referrer` caused form `Origin: null`, and an authenticated paired browser without a cached owner fell through to local status instead of remote readiness. The remote router now uses `same-origin`; null/foreign origins remain rejected. Paired workspace capture uses remote readiness and rejects later recovery-generation changes before private work. Focused web security tests passed six checks (`/tmp/mxr-pool3-web-focused-20261106.log`); the owner regression passed (`/tmp/mxr-pool3-browser-owner-regression-red.log`, historical filename).

Run g verified native WSS accepted/unknown send recovery and browser login/search/read. Run h recorded 55 product checks, including exact uploaded attachment bytes and one send, preview-equal archive, live reconnect without reload, CSRF/host-administration denial and revocation while the second client continued. Its receipt remains failed because the parent waited before closing the browser child's stdin. Neither diagnostic policy overrides nor a partial receipt counts as final acceptance. Evidence: `/tmp/mxr-remote-browser-20261106-acceptance-{g,h}/result.json` and the corresponding retained `.log` files.

Native desktop compilation and its 24-check parser/navigation/generated-JavaScript harness passed earlier. Client/config/protocol library suites passed 58/72/102 before fresh-main integration and the last owner/status changes; assess only affected reruns. No live supervisor, Tailscale mapping, system trust store, provider mailbox or migration has changed. Live macOS/Linux installation, crash/reboot and actual native WebView acceptance require their own authorized target environment. Do not claim a release, installed supervision or real mailbox migration from this repository candidate.
