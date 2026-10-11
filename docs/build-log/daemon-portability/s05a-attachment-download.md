# S05a: download and open attachment bytes on the client

The transfer contract adds path-free metadata and bounded chunks to IPC. Metadata carries message and attachment IDs, filename, MIME type, exact byte length, SHA-256 and the maximum raw chunk size. Chunk responses echo message ID, attachment ID and offset and carry standard base64; requests above 256 KiB, zero-length chunks, offset overflow and ranges past EOF fail. The status capability defaults false so an older daemon returns a clear unsupported error before a client starts the transfer.

The shared Rust client verifies IDs, chunk shape, decoded lengths, total length and SHA-256. It writes a private temporary file beside the final path, then exposes the verified file atomically. CLI downloads use no-clobber behavior; TUI replacement follows the existing save-dialog confirmation. Open operations download to the client cache and invoke the client-side opener. The bridge has new authenticated typed metadata/chunk routes; the existing path-returning open/download routes remain for deployed local clients. MCP exposes metadata and bounded 64 KiB chunk tools with an optional account assertion.

The browser assembles and verifies the complete byte array before creating a Blob. Open uses a synchronously opened, detached tab only for `text/plain` and PNG/JPEG/GIF/WebP; unsupported types and popup-blocked opens download instead. HTML, SVG and PDF never enter the inline-open path. This keeps file bytes on the browser client and avoids a second binary streaming route.

The daemon still fetches each attachment as a complete provider `Vec<u8>`. The browser also holds the complete transfer in memory before creating its Blob. There is no total attachment-size cap in this slice. These are existing provider and browser memory limits, not guarantees of streaming end-to-end.

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
- In `apps/web`, `npm run gen:types`, `npm run typecheck`, and `npx vitest run src/features/mailbox/attachmentTransfer.test.ts` passed; 5 browser transfer tests passed.
- Targeted `oxlint` and `oxfmt --check` passed with four threads. `anti-slop-check` found no issues in the transfer implementation or its tests; it reports four standard generated `headers: { [name: string]: unknown }` entries for the new OpenAPI operations.

Regenerate browser types from the bridge source with:

```sh
cd apps/web
npm run gen:types
```

`agent-work run -- scripts/pre-pr-rust-gate --base HEAD` passed (task 5815): Rust formatting, scoped Clippy across the affected crates with all targets/features and `-D warnings`, architecture-boundary check, and cargo-deny. Cargo-deny reported existing unmatched skip roots, duplicate lock entries, and yanked locked crates; advisories, bans, licenses, and sources passed. The gate leaves workspace CI checks to CI. For web lint or formatting, pass `--threads=4` to oxlint/oxfmt in this workstation environment.

The canonical cross-boundary proof remains root-owned: a client under filesystem denial must transfer a deterministic attachment larger than one chunk through the selected daemon, verify exact bytes and hash, and leave the client with no usable partial destination on injected failure. Product release and installed-client acceptance are separate gates after this code checkpoint.
