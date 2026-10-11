# Archive outcomes need reconciliation and retention policy

**Status:** open · **Found:** S04 candidate `51a977b2822e723d488c0c7a6b99123932091176`

## Unknown and provider-succeeded outcomes stay unresolved

The Archive runner commits `unknown` before the provider call and never repeats
that item under the same operation. An explicit provider rejection becomes
`not_executed`; a successful provider effect is recorded as `provider_succeeded`
before local Inbox-label reconciliation. If provider acceptance is ambiguous,
the receipt remains `unknown`. If local reconciliation fails after confirmed
provider success, it remains `provider_succeeded`. The current query and
same-identity submit surfaces do not settle either state through another
provider call.

Design a later evidence-based reconciliation path that can resolve these
receipts without repeating the provider effect. Do not infer acceptance from
error text or promise provider idempotency.

## Document the local persistence performed by Archive preview

On candidate `f3986183c92f532e125304cb15c3a70eb9c44c7c`,
`PreviewArchive` is grouped with read requests in
`crates/daemon/src/handler/mod.rs#L2064-L2067`, but
`crates/daemon/src/handler/archive_recovery/mod.rs#L173-L229` stores a retained
review. `GetArchiveWorkspace` also calls `archive_workspace()`, which inserts
the workspace identity on first use
(`crates/store/src/mutation_jobs.rs#L46-L60`). The classifier's `Read` contract
says requests that persist anything should not use that class
(`crates/daemon/src/handler/mod.rs#L1980-L1991`). Neither operation dispatches
a provider action or changes mail. Resolve the classification policy and make
local persistence and retention explicit where Archive is described as a
preview or read operation; keep the distinction between no mail effect and no
local write clear. This is a policy mismatch, not evidence of secret exposure.

## Measure the Archive capability check on the polling path

On the same candidate, `IpcConnection::request_response` asks the daemon for
`GetStatus` before each Archive workspace, preview, submit and outcome request
(`crates/client/src/lib.rs#L271-L308`). Status collects a database-backed
snapshot, checks whether search repair is needed, and requests semantic runtime
status (`crates/daemon/src/handler/diagnostics/mod.rs#L1185-L1225`). The client
polls a running Archive operation every 50 ms
(`crates/client/src/archive.rs#L217-L243`); the browser polls every 100 ms
(`apps/web/src/features/mail-actions/archiveRecovery.ts#L163-L168`). Measure
this path before changing it, then evaluate caching capability per connection
or avoiding the status probe on repeated outcome polls. No latency or load
estimate is established here.

## Stop dispatching an account's remaining batch after an unknown result

The runner records `unknown` for an ambiguous provider result or timeout, then
continues its item loop. Later `pending` items for that same account can still
be dispatched; the current path has no same-account fail-fast guarantee
(`crates/daemon/src/handler/archive_recovery/runner.rs`). The default provider
classifier maps errors to `Unknown` (`crates/core/src/provider.rs`); the
candidate has no Gmail or IMAP classifier override. Evaluate stopping further
dispatches for that account after the first unknown result or timeout, leaving
its remaining items `pending` while allowing other accounts to proceed. Do not
assume provider idempotence across adapters.

## Clear the in-memory running marker after a runner panic

The spawned task removes its operation key after the normal `runner::run`
result handling (`crates/daemon/src/handler/archive_recovery/mod.rs`). A panic
in the runner would skip that cleanup, leaving the operation marked running in
memory until daemon restart. No such panic was observed. Evaluate a drop guard
or task supervision so cleanup also runs when the task unwinds.

## TUI outer timeouts hide the retained operation ID

The TUI IPC worker's outer request timeout returns a generic message that the
request may or may not have been applied (`crates/tui/src/ipc.rs`). For mailbox
Archive, the operation is already retained in the local journal, and the
command palette can list its query and submit actions
(`crates/tui/src/client.rs`, `crates/tui/src/ui/command_palette.rs`). Surface
that recovery route in the timeout guidance so the user knows to query the
retained operation instead of starting another Archive action. Keep the
unknown outcome rule: query it, and do not repeat an unknown provider effect.

## Cancelling browser Archive review leaves the selection cleared

The browser clears destructive-action selection before it opens the Archive
review confirmation (`apps/web/src/features/mail-actions/mailMutations.ts`).
Cancelling retains the reviewed operation but prevents submission
(`apps/web/src/features/mail-actions/archiveRecovery.ts`). Make the
post-cancellation state clear, or restore the selection, so the user does not
have to infer why the messages disappeared from the selection. Cancellation
does not dispatch the provider action.

## Ignore a second list Archive while the first review is unresolved

On candidate `80457e31199d31f73c9dc891fbb17b343efc27b3`, Archive does not add
the row to `pendingMailOps`, and the list verb path has no per-message
Archive-pending guard (`apps/web/src/features/mail-actions/mailMutations.ts#L52-L60`,
`apps/web/src/features/mail-actions/mailVerbs.ts#L86-L114`). Pressing `e` again
while the row remains visible can open a second confirmation for that same
message; accepting it claims the newest `u` slot. The first action's toast
still has its own Undo (`apps/web/src/features/mail-actions/mailUndo.ts#L179-L192`),
but keyboard Undo can now belong to the second action. If the second review
sees the message already outside Inbox, the runner completes it without a
provider call or Undo snapshot
(`crates/daemon/src/handler/archive_recovery/runner.rs#L159-L170`,
`#L253-L270`). Avoid offering a second list Archive for a message with an
unresolved Archive review, or keep the row visibly pending without removing it.
This can obscure which action `u` will undo; the observed path does not repeat a
provider effect.

## Show useful message details in the browser Archive confirmation

On candidate `f3986183c92f532e125304cb15c3a70eb9c44c7c`, the native confirmation
shows the count and message UUIDs
(`apps/web/src/features/mail-actions/archiveRecovery.ts#L155-L162`). Evaluate
presenting subject and sender details so the user can recognize the reviewed
messages. Keep submission bound to the original server-reviewed selection;
presentation must not replace or rebuild that selection.

## Retained receipts have no cleanup policy

`recoverable_archive` rows are excluded from mutation-job pruning, and the
local Archive journal stores one file per operation with retain, load and list
operations but no deletion path. The CLI and TUI share that file journal; the
browser keeps operation records in `localStorage`. There is no user-facing
cleanup command or age limit. Retention happens before submission, so a local
storage failure prevents the client from submitting. Each retained update
serializes and replaces the full per-operation receipt; its cost has not been
measured. Define a cleanup policy that preserves unresolved outcomes and any
recovery identity still needed by clients.

## Evaluate power-loss durability for effect receipts

The store configures WAL with `synchronous=NORMAL` for writer and reader
connections (`crates/store/src/pool.rs`).
[SQLite's synchronous pragma](https://www.sqlite.org/pragma.html#pragma_synchronous)
says a committed transaction in WAL mode with `synchronous=NORMAL` may roll
back after an operating-system crash or power loss. Evaluate effect-receipt
synchronization before making a stronger durability promise; preserve the
current daemon-process crash and restart scope unless that work changes the
evidence.
