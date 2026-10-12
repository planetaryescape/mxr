# Attachments transfer to the consuming client

The transfer contract adds path-free metadata and bounded chunks to IPC. Metadata carries message and attachment IDs, filename, MIME type, exact byte length, SHA-256 and the maximum raw chunk size. Chunk responses echo message ID, attachment ID and offset and carry standard base64; requests above 256 KiB, zero-length chunks, offset overflow and ranges past EOF fail. The status capability defaults false so an older daemon returns a clear unsupported error before a client starts the transfer.

The shared Rust client verifies IDs, chunk shape, decoded lengths, total length and SHA-256. It writes a private temporary file beside the final path, then exposes the verified file atomically. CLI downloads use no-clobber behavior; TUI replacement follows the existing save-dialog confirmation. Open operations download to the client cache and invoke the client-side opener. The bridge has new authenticated typed metadata/chunk routes; the existing path-returning open/download routes remain for deployed local clients. MCP exposes metadata and bounded 64 KiB chunk tools with an optional account assertion.

The browser assembles and verifies the complete byte array before creating a Blob. Open uses a synchronously opened, detached tab only for `text/plain` and PNG/JPEG/GIF/WebP; unsupported types, popup-blocked opens, and tabs closed during transfer download instead. HTML, SVG and PDF never enter the inline-open path. This keeps file bytes on the browser client and avoids a second binary streaming route.

The daemon still fetches each attachment as a complete provider `Vec<u8>`. The browser also holds the complete transfer in memory before creating its Blob. There is no total attachment-size cap in this slice. These are existing provider and browser memory limits, not guarantees of streaming end-to-end.

## Review corrections

The TUI save modal carries replacement authority only after the user confirms an already-existing destination. An unconfirmed save uses atomic no-clobber publication, so a file created while bytes are arriving is preserved. Open may replace its client-local cache copy. The CLI table output keeps the selected attachment's actual one-based index; JSON/JSONL remain unchanged. MCP attachment fetching is network-bound and classified as a mutation, so its profile needs `safety_policy = "full"` and `allow_destructive = true`; `allowed_accounts` remains enforced and `allow_send` can stay false.

## Cache persistence

`insert_body_tx` replaces all attachments for a message. A stale full-body write from concurrent downloads could erase a sibling attachment's newly cached path. Cache materialization therefore serializes misses per `(message_id, attachment_id)`, creates private `0600` temporary files before writing bytes on Unix, atomically renames to a unique cache filename, then updates only that attachment row and its size. Idle lock keys are pruned on later acquisitions. If the row disappeared, the new file is removed. Tests cover sibling-row preservation, missing IDs, and two simultaneous first downloads sharing one provider fetch and one published cache path.

## Validation

Starting base was `f2e522c882599127d6fae9fbe5a0855f542472b4`. Focused results on the candidate tree:

- `scripts/cargo-test -p mxr-client --lib attachment_transfer`: 4 passed, including 0600 mode while bytes are still partial, cancellation cleanup, integrity failure preserving an existing confirmed target, and no-clobber behavior.
- `scripts/cargo-test -p mxr-store --lib attachment_cache_update_changes_only_the_selected_attachment`: 1 passed.
- `scripts/cargo-test -p mxr-mcp --lib attachment_chunk_tool`: 2 passed (64 KiB limit and bounded dispatch).
- `scripts/cargo-test -p mxr-web --lib attachment_transfer_routes_return_path_free_typed_responses`: 1 passed.
- `scripts/cargo-test -p mxr --lib attachment_materialization_serializes_same_attachment_and_prunes_idle_keys`: 1 passed.
- `scripts/cargo-test -p mxr --lib dispatch_download_attachment_persists_local_path`: 1 passed; concurrent provider calls were gated and counted, and both responses shared the persisted cache path.
- In `apps/web`, `npm run gen:types`, `npm run typecheck`, and `npx vitest run src/features/mailbox/attachmentTransfer.test.ts` passed; the suite now has 6 browser transfer tests, including a popup closed during transfer. Targeted `oxlint` and `oxfmt --check` passed with four threads.
- Follow-up review regressions passed: `scripts/cargo-test -p mxr-tui --lib save_modal_carries_only_explicit_overwrite_confirmation` (task 5821), `scripts/cargo-test -p mxr-client --lib no_clobber_preserves_a_destination_created_during_transfer` (task 5822), and `scripts/cargo-test -p mxr --lib transfer_table_keeps_the_selected_attachment_index` (task 5823).
- Generated CLI help passed `scripts/cargo-test -p mxr --test cli_help`. The updated site passed its three tests, validation of 227 OpenAPI paths, and an Astro build of 187 pages. Local rendering initially lacked Astro; installing locked dependencies with lifecycle scripts disabled resolved it without changing the workstation script policy.
- Targeted `oxlint` and `oxfmt --check` passed with four threads. `anti-slop-check` found no issues in the transfer implementation or its tests; it reports four standard generated `headers: { [name: string]: unknown }` entries for the new OpenAPI operations.

Regenerate browser types from the bridge source with:

```sh
cd apps/web
npm run gen:types
```

`agent-work run -- scripts/pre-pr-rust-gate --base HEAD` passed (task 5815): Rust formatting, scoped Clippy across the affected crates with all targets/features and `-D warnings`, architecture-boundary check, and cargo-deny. Cargo-deny reported existing unmatched skip roots, duplicate lock entries, and yanked locked crates; advisories, bans, licenses, and sources passed. The gate leaves workspace CI checks to CI. For web lint or formatting, pass `--threads=4` to oxlint/oxfmt in this workstation environment.

## Consuming clients passed the candidate journey

Root acceptance used source `b55994bdc352cd95db893ceda95e77a065cc20ee`, built with the embedded web UI. The binary SHA-256 was `6742fde492fdeb822e67123d8edfdf46239d53809e465d83a0a047da6759c7c9`.

- CLI transferred 1,188,000 deterministic bytes through `cmd://` while macOS denied reads of the daemon data directory. The SHA-256 matched, a repeated download preserved the destination, and the client created no fallback database or socket.
- MCP assembled the same bytes from 19 chunks. Missing mutation permission, invalid ranges and a wrong account assertion were rejected.
- The real TUI saved the same bytes under filesystem denial. The first Enter preserved an existing file; the second confirmed replacement. Open copied a separate synthetic text attachment to the client cache and the OS opener returned success. External editor contents were not inspected.
- The real browser downloaded a 1,280,116-byte synthetic HTML attachment using metadata and five chunks. Its hash matched while the bridge process could not read the daemon cache file. Open downloaded active HTML content without opening another tab. Browser errors and console logs were empty. Raster-image Open was not exercised.
- A new client against the published v0.6.77 daemon rejected the unsupported capability without a host-path fallback. The published v0.6.77 CLI still downloaded through the candidate daemon using the legacy local path contract.

The [sealed candidate receipts](evidence/s05a-candidate-b55994bd/README.md) preserve these results and their source hashes.

All fixtures used synthetic mail. Their owned daemons, bridge processes and browser session were stopped; fixture tokens were removed. The initial CLI fixture failed because `sync --account` resolves against the client configuration. Omitting the selector on the isolated single-account fixture corrected setup. An initial TUI fixture exited during the attempt; the repeated journey used a detached, explicitly tracked fixture daemon. These initial attempts did not establish a transfer result.

The final scoped pre-PR gate passed in 3m10.5s (task 5840). CLI help, schema generation and validation passed in task 5847; the site render passed in the follow-up locked-dependency job.

The candidate was then rebased onto release metadata at `f56ac1527787ca312bda7b52935947ee94495251`. Product sources are identical; TOML comparison verifies only workspace package versions changed from 0.6.78 to 0.6.79, with the dependency graph unchanged. CI and published-release acceptance remain required before this stage is complete.

## CI exposed three test maintenance gaps

At head `243eb46c`, CI38180914741 reported one failure out of 957 web unit tests: the cold Inbox assertion inspected rendered text immediately after query state paused. Its DOM still showed the loading skeleton, whose existing hook holds a displayed skeleton for at least400ms. Awaiting the visible status removes that race without changing product behavior. The unmodified full suite passed locally; after the assertion fix all957 tests passed again, with targeted lint/format checks clear.

OpenAPI conformance38180914789 passed route integration but found four missing attachment schema names in the stored summary snapshot. Regenerating from the live schema added exactly those names. All128 mxr-web library tests then passed. These two corrections change tests only.

The existing browser attachment smoke also mocked the legacy path-returning endpoints. CI38181360481 passed 384 browser cases and failed that case. Its correction exercises path-free metadata and four chunks per action, then verifies exact bytes for both PDF Open fallback and Download. The focused test passed locally with the repository's `PW_CHROME` override selecting installed Chromium; CI used its pinned browser. Full CI38182827017 passed at `54a4477e7019738592c5ed4b1f3be442b62fe147`, including the browser suite and production speed gate. The run took 15m32s; browser tests accounted for 12m24s.

PR395 merged at `4705e7584ca267e18ff3447a16fe3b1046c17ce7`, whose tree `45210000c817294584c33bfec8716ea1b66b16c3` matches the reviewed candidate. Independent review remained waived by BK while Anthropic was unavailable. Release PR399 passed CI and merged at `0067e774fc690f67df481f9f9ef4568d3d4ff442`. The v0.6.80 release workflow passed in 31m18s. Both platform archives and their checksums were verified; the installer and Homebrew matched the macOS binary. Notarization was skipped by the release workflow.

## Published acceptance found a missing-directory regression

The [published v0.6.80 packet](evidence/s05a-installed-v0.6.80/README.md) records passing CLI, MCP and browser transfers. Browser Open downloaded synthetic HTML without another tab and opened synthetic text in a passive `text/plain` tab; exact bytes matched in both cases.

The real TUI failed to save into a missing Downloads directory. Source `0067e774` selected the parent and opened a private partial file without creating the parent; the previous daemon-side save created it. PR404 adds parent creation after metadata and destination validation. Six focused transfer tests, client formatting and client Clippy pass at `71cb4362ec22966db0ab77cb302025bfab077094`. The tests include missing nested directories and corrupt multi-chunk cleanup. The full application build passed in 3m21s. Root drove the real TUI through a native PTY with daemon-data reads denied: saving to the absent Downloads directory created it and wrote exactly 1,188,000 bytes, SHA-256 `5b63e9ce9b6d3979b2993d87ffef9cce98a63846cc5b8770c2b362cbabe0fd16`, mode `0600`. CI38186896091 passed. PR404 merged as `9c5c306adda575ec622694eb53ec72fa97f09b83`, matching reviewed tree `ced715a5d92d6f4547ff6e2414d483d028ffc685`. Release PR405 proposes v0.6.82; its metadata changes only workspace versions and the changelog. S05a remains active until the corrected published binary passes the original TUI journey.


## Measured waiting is separate from implementation

| Phase | Observed interval or duration | Evidence |
| --- | --- | --- |
| Assignment to first feature checkpoint | 21:18:11–21:56:45 UTC, 38m34s on11 October | Worker brief timestamp and original commit author timestamp |
| Root correction checkpoint | 22:08:34 UTC | Original overwrite-consent correction commit |
| Final scoped Rust gate | 3m10.5s | Pueue task5840 |
| Successful feature CI | 23:03:28–23:19:00 UTC, 15m32s | Run38182827017 |
| Browser tests within that CI | 12m24s | Web E2E test-step timestamps |
| Published release | 23:27:27 to 23:58:45 UTC, 31m18s | Run38184347088 |

The first interval includes implementation, local checks and their waits; it is not measured keyboard or model time. No reliable total for active agent time or BK attention was retained. One consolidated root review found the overwrite-consent defect; subsequent CI corrections repaired three stale or timing-sensitive test fixtures. Existing attachment E2E fixtures should be updated when their client endpoint changes, before the first whole-suite run. No measured speedup is claimed for that change in practice.
