# Saved drafts retain the bytes the client uploaded

The S05b candidate starts from `origin/main` source
`5007eb3f95484ec3a067fe5d3126d3aa8d1fa9d3` (tree
`43269888f7c907d6c77043ee65265e33259f8e45`). Root owns the checkpoint commit,
upstream integration and published-client acceptance. Resource migration 078
reserves 077 for the incoming categories change. Candidate edits remain
uncommitted until that checkpoint.

## Uploads have a bounded daemon resource contract

`Draft.managed_attachments` stores immutable resource metadata separately from
legacy `Draft.attachments` paths. Resources carry a typed opaque ID, owner draft
and account, filename, MIME type, exact length and lowercase SHA-256. Begin
requires a saved, editable owner. Preview validates a declaration without
creating an owner, file or metadata row. Raw chunks are at most 256 KiB; MCP
accepts at most 64 KiB. A resource is limited to 25 MiB. This is a local disk
policy; it does not set provider limits or change legacy path behavior.

```sh
mxr compose --to alice@example.com --subject 'Portable attachment' \
  --body 'Please read the attachment.' --attach ./notes.txt --draft --format json
mxr drafts attach DRAFT_ID ./another.txt --dry-run --format json
mxr drafts attach DRAFT_ID ./another.txt --expected-revision REVISION --format json
mxr send DRAFT_ID --dry-run --format json
```

Shared Rust client preparation capability-checks before opening a new client
file, saves the owner, uploads bounded chunks, then CAS-updates its resource set.
CLI compose/reply/forward select their send identity after the resource save.
TUI preparation selects and retains that canonical revision before submission.
The provider receives a resolved clone containing daemon-local paths; stored
drafts and reviewed command hashes retain the portable metadata.

## Omission cannot silently remove managed attachments

An absent managed field identifies an old-wire caller; an explicit empty set
removes resources. Updates and safety checks from a caller omitting a stored,
nonempty set fail with an upgrade instruction. Stored send and scheduling
requests carry a default-false review marker, so an old caller cannot transmit
attachments invisible in its review. Download and upload capabilities remain
independent: v0.6.82 downloads still work against a daemon advertising upload
support as absent/false.

Legacy path drafts remain readable and sendable. Their path-backed bytes stay
mutable. Opening a migrated store with an old binary is unsupported: old
binaries predate the managed field and cannot enforce this contract. Keep the
current binary and forward-fix problems; downgrade only with a pre-migration
backup and its matching binary.

## File publication precedes ready metadata

The per-draft operation guard covers upload begin/write/finalize, set validation
and CAS, cleanup rechecks, and sending through Sent ingestion. No SQLite
transaction remains open during client input or provider waits. Chunks append
sequentially, and a repeated committed range must contain the identical bytes.
Finalize verifies length and digest, syncs the file, publishes it under a private
resource directory, syncs the directory, then marks metadata ready. A retry
recognizes and verifies a published file left by a crash before the ready write.
Finalized files are read-only on Unix; their directories are private.

Preview and send resolve the same resource metadata. Send rehashes actual files
before claiming submission. Missing or corrupt bytes have no provider effect.
Scheduled revisions become stale when the resource set changes. Sending,
uncertain and accepted-but-not-ingested drafts remain owners and retain bytes.

Cleanup runs on new upload begins. It considers at most 32 resources older than
seven days whose owner draft is absent and which have no unknown or accepted-not-ingested
durable send outcome, then rechecks both references under that draft's guard
before removing files and metadata. Existing owners retain even
abandoned partial uploads. A stopped daemon leaves files for the next begin;
there is no new cleanup scheduler.

```sh
scripts/cargo-test -p mxr --lib draft_attachment_resources
scripts/cargo-test -p mxr-client --lib portable_upload
scripts/cargo-test -p mxr-mcp --lib draft_attachment_tools
```

## The first daemon proof observed provider bytes

Pueue task 6004 queued at 02:07:55+01:00 and started at 02:08:53+01:00
(58 seconds waiting). Cargo compilation took 3m07, seven seconds over the
assigned three-minute build budget. All five resource tests passed in 2.81s:

- `draft_attachment_resources_chunks_retries_cas_and_provider_bytes`
- `draft_attachment_resources_bounds_ownership_and_incomplete_finalize`
- `draft_attachment_resources_empty_duplicate_corrupt_and_review_invalidation`
- `draft_attachment_resources_published_file_crash_retry_and_cleanup_retention`
- `draft_attachment_resources_restart_sends_original_client_bytes`

The restart test reconstructs AppState and SQLite from the same persisted store,
changes and deletes the synthetic client file, previews the saved resource, then
sends it. The fake provider reads the actual attachment file at its send
boundary; its witness digest and length match the original bytes. The durable
Sent receipt is queried afterward. This proves daemon persistence and provider
input, and does not replace root's actual released CLI/TUI/browser/MCP journey.

The fake spool acceptance JSON now records per-attachment filename, length and
SHA-256, with no bytes or body. Production providers continue using
`MailSendProvider`; existing whole-buffer MIME construction remains a limit.

## Root checkpoint still owns release verification

Next, verify the final source tree after the scoped client/MCP/browser tests,
regenerate web types, OpenAPI and CLI help, run the scoped pre-PR gate, and build
`mxr`. Root integrates freshly fetched upstream, reviews that exact checkpoint,
and owns CI, release-please and installed-client acceptance. No personal mail,
existing daemon restart, external deployment, Git operation or shared ledger
edit occurred in the worker.

## Browser and client checks reached the checkpoint

Task 6007 compiled in 2m43 after about one second in the queue. It passed
`portable_upload_rejects_old_daemon_before_opening_client_file`,
`portable_upload_sends_bounded_bytes_and_preserves_selected_send_identity`, and
`draft_attachment_tools_preview_without_writes_and_bound_raw_chunks`. Its
`draft_attachment` filter matched no compose or web test; it is not browser
coverage. Task 6013 separately passed
`compose_attachment_upload_returns_managed_resource_and_revision`, including
reload of the finalized revision and resource token, in 2.76s after 51.79s
compilation. TypeScript typecheck, targeted strict oxlint, and anti-slop checks
passed.

The browser hook regressions cover bounded chunks, the new revision surviving
the next edit, completion after switching drafts, and an unsupported daemon
refusing before reading client bytes. The final compose suite is run under
bundled Node 24.19.0, matching CI's Node 24. On PATH, Node 26.9.0 failed twelve
existing ComposeHost send tests; the first reproduced test passed under the
bundled runtime. The test probe was restored verbatim. No product or test
compatibility workaround was added for that runtime difference.

Root requested a pause before the final build to integrate fresh main once.
The queued final resource regression run is task 6021. Its new
`draft_attachment_resources_pre_upgrade_hash_replay_preserves_unknown_and_accepted`
test persists the pre-S05b field-omitted, struct-order command hash and checks
None and explicit-empty resource sets against both retained outcomes, with no
provider invocation. Generated web types/OpenAPI/help, editor and safety
focused checks, scoped pre-PR hygiene and the final binary build remain for
the freshly integrated tree. Root's `s05b_cli_accept.py` owns the actual
reciprocal-filesystem-denial CLI journey.

Task 6021 passed all six resource tests in 2.01s after 3m16 compilation and
108 seconds waiting (02:27:20+01:00 queued, 02:29:08+01:00 started). The
pre-upgrade hash regression passed both outcome states and both empty-field
representations. All 42 compose JavaScript tests passed under Node 24.19.0.

A final retention correction excludes owner-absent unknown/accepted-not-ingested
outcomes from cleanup and rechecks the outcome under the draft guard. This
covers an owner row removed by an account cascade; the outcome is still a
durable reference. The added
`draft_attachment_resources_cleanup_retains_owner_absent_inflight_bytes` and
`draft_attachment_resources_concurrent_sets_have_one_cas_winner` tests are not
yet run; they belong to the final integrated resource gate. No compilation is
running at this handoff. Root must checkpoint and integrate upstream before
the remaining gates.

The replay choke point returns a matching retained outcome before consulting
resource liveness. Completed resources may have been cleaned after seven days;
that cannot turn an accepted replay into a missing-resource failure. The
existing provider-byte test now also removes completed resource metadata/files
and asserts the same `SendDraft` identity returns its receipt without another
provider invocation. That extension awaits the integrated resource gate.
