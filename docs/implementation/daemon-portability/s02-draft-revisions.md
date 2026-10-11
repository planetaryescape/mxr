# Draft revisions preserve both editors' text

Save a draft, restart the daemon, reopen it and edit it from two clients. One
current edit is accepted; a stale editor keeps its unsent text and receives a
revision conflict. Daemon IPC, CLI, TUI, browser and MCP use the same stored
draft revision. [The delivery ladder](orchestration.md) tracks release status.

## Stored revisions guard edits before provider effects

A local revision starts at 1 and advances through SQL compare-and-set updates.
It is separate from the provider revision. Save and update return the canonical
saved draft; list and read expose its revision. Repeating creation with the same
identity and content is safe; different content conflicts.

Update, delete and existing-draft provider sync check the expected revision
before provider effects. Provider sync uses saved canonical content. Content
and status writers acquire the draft lock before the account provider lock.
Reconciliation re-reads the link in that order and advances the local revision
when it pulls provider content. Send and orphan recovery share the draft lock;
cached reply headers retain the current content revision.

The store assumes one active daemon owns it. SQL comparison guards content
writes; process-local locks serialize provider effects. This does not coordinate
multiple daemon processes writing the same database.

## Each editor retains its own recovery content

CLI and TUI editors use unique scratch files. Conflicts and editor, parse,
validation or transport errors report the retained path. Reopening a draft
cannot overwrite a previous editor's scratch file.

Browser autosave keeps one draft identity in private session metadata. The
bridge saves the submitted snapshot before replacing its shared compose file.
A conflict preserves the browser buffer and a private recovery file with the
editor's original observed revision. Only an operation's accepted response may
advance that editor's revision; another writer's current revision is not proof
that these edits were saved.

Recovery autosaves and Close preserve content locally. They do not repeatedly
submit it to canonical storage. Explicit Save or the visible Retry button
submits the original observed revision: an existing draft uses conditional
update; a first save reuses its original draft identity. A conflict or ambiguous
response keeps the latest private content. Successful retry adopts only its
own response and retains any edits typed while the request was running.

Reconnect keeps the preservation warning until canonical save succeeds. The
Saved timestamp is cleared while content is only preserved locally. Open saved
draft explicitly loads canonical content and retains an Open preserved edits
action. Reload actions recheck identity and intervening text before adoption.

Each conflict copy or explicit recovery open owns separate copies of uploaded
attachment bytes. Frontmatter, metadata and the response retain their order;
rewriting the header preserves raw body whitespace. Equal basenames remain
distinct. Recovery Discard removes only its marked file and owned attachment
directory. Earlier recovery copies and user-managed external files remain.
A failed copy removes its incomplete destination and retains the source.

Accounts can be selected before first persistence; a saved draft keeps its
account. Existing HTML content stays read-only in the browser composer.
Incomplete autosaves remain possible, while explicit save and effectful actions
validate the submitted content. Send, safety, schedule and provider-copy paths
check the observed revision against the same parsed snapshot they will use.
A recovery must first be explicitly saved before those actions proceed. Both
MCP provider-save names require the revision returned by their preview.

## Upgrades require clients to reopen

New clients require `draft_revision_supported` before draft writes. New daemons
reject unconditional edits and deletes. Reads remain compatible. An older
binary cannot enforce revision safety, so downgrade is unsupported; use a
forward fix.

Close and reopen clients around an upgrade. Old running clients cannot safely
write, and an old creation handler may reject the new canonical response after
creation commits. A pre-upgrade browser window lacks revision metadata: retain
its text and copy it into a newly opened composer before saving. No revision is
inferred for that window.

Reviewed-send tokens, recoverable send outcomes and uncertain provider effects
belong to S03. S02 does not promise exactly-once delivery or change the existing
provider-effect/SQL crash boundary. Portable attachment resources belong to S05;
attachments here still use host paths. Explicit recovery opens may retain
additional private copies; automatic saves reuse their current destination.

## Synthetic journeys verify persistence and recovery

Run the built or released binary without real accounts:

```sh
scripts/smoke-draft-revisions /absolute/path/to/mxr
```

The script saves revision 1, restarts and reads it, accepts editor B at revision
2, rejects editor A's stale revision, prints its preserved text and previews
deletion. HOME, XDG, configuration, database and socket are isolated; shutdown
targets only the owned child.

Use Node 24, matching CI, for frontend checks. The Mac comparison on unchanged
`6c9a22e8f1eab335e7912f346188f1251efe2e49` passed all 883 tests on Node 24.21.0;
Node 26.9.0 failed 36 tests around unavailable `window.localStorage`. No product
source changed for the runtime comparison.

Focused checks use the shared build queue and at most four workers:

```sh
scripts/cargo-test -p mxr -p mxr-web -p mxr-store -p mxr-client -p mxr-compose -p mxr-mcp -p mxr-tui --tests draft
scripts/cargo-test -p mxr-web --lib compose_and_scheduling
scripts/pre-pr-rust-gate
cargo build -p mxr
npm run typecheck --prefix apps/web
npm test --prefix apps/web -- src/features/compose src/features/drafts --maxWorkers=4
npm run gen:types --prefix apps/web
```

Race tests hold the account provider lock, observe draft-lock contention, then
queue a send or stale deletion. Browser coverage includes overlapping autosaves,
session switches, edits during Save, real Close/Discard/reopen, first-save and
saved-draft retries, owned attachment copies, reconnect warnings and visible
Retry. The isolated production-browser journey at the SHA above retained the
stale editor's text and attachment, preserved the winner through Discard, and
saved outage recovery only after explicit Retry. Its request trace contained
no send, schedule or provider-save calls.
