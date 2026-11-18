# Microsoft linked-task follow-ups

Status: open, nonblocking. These items come from an independent review of
`11ea9bb5afc1e4de8836ca3fbde7110763465d63`. Each one was rechecked in the code
at `856576c87`. They are source-audit findings, not reports from a real account.
Line numbers refer to that commit.

## The link endpoint documents 400 but returns 422

`mail_microsoft_todo_link` (`crates/web/src/openapi.rs` L811) documents a stale
preview or a missing CLI as HTTP 400. The daemon returns `InvalidRequest` for
both (`crates/daemon/src/handler/microsoft_todo.rs` L142-L145). The bridge maps
`InvalidRequest` to HTTP 422 (`daemon_error_kinds_map_to_http_statuses`,
`crates/web/src/tests.rs` L5385).

Correct the annotation, `site/public/openapi.json` and
`apps/web/src/api/generated.ts`. Add an HTTP-level test for a stale fingerprint.
The web client already treats every non-success status as a failure, so users
see the error today.

## An interrupted check looks like a link error

`background` writes "Checking the linked Microsoft task…" to `last_error` before
each attempt (`crates/daemon/src/handler/microsoft_todo.rs` L331). If the daemon
stops during the attempt, that text stays. `link_with_cli` then reports the link
as `attention` (L119) until a later pass replaces the text.

Store progress apart from errors, or show it differently. Keep uncertain-write
receipts. Never treat a stale progress message as permission to retry a create
or a write.

## Explicit sync has no overall bound

The background pass takes at most eight links (`microsoft_todo.rs` L325). An
explicit sync of all links takes every link and runs them one at a time
(`select_links`, `sync_links`). Each CLI call has a 30-second bound
(`crates/daemon/src/handler/microsoft_todo_cli.rs` L333), but the request has
none. The request also holds the sync gate (L214), so background passes skip
while it runs. Many unresponsive peers can make one request take minutes.

Add paging or per-item progress. Do not change the preview and apply selection,
widen account scope or replay uncertain writes. Test with a synthetic slow CLI
and many links. Do not use real tasks.

## An early child exit loses the refusal

`MicrosoftTodoCli::run` writes the context to stdin (`microsoft_todo_cli.rs`
L287), then joins the write, read, drain and wait steps with `try_join!` (L315).
If the child closes stdin before the write finishes, the write fails with
`CliError::Unknown` and `try_join!` returns before mxr reads the exit status. A
definite refusal (exit 2 to 7) then looks uncertain. The receipt pauses, so mxr
does not retry blindly or create a second task.

Add a fixture that closes stdin early and exits with a refusal. Keep the refusal
classification only where the exit status proves that no write happened. Do not
accept a partial write that exits with success, and do not release an uncertain
receipt on a guess.

## A slow TUI sync blocks other requests

The TUI sends Microsoft requests through `ipc_call` (`crates/tui/src/runner.rs`
L2492), which queues them on the shared serial IPC worker. The worker allows each
request 60 seconds (`crates/tui/src/ipc.rs` L18) and reconnects without replay
after a timeout. A slow sync delays every request queued behind it. Durable
receipts still protect writes.

Use a dedicated bounded connection with the original optional workspace owner.
Test ownerless sessions, delayed peers, unrelated requests and uncertain
outcomes before you change connection ownership or cancellation.

## The install link points at a missing anchor

`INSTALL_URL` (`microsoft_todo_cli.rs` L14) and
`site/src/content/docs/guides/todo.md` L198 link to
`https://github.com/planetaryescape/ms-todo#installation`. The ms-todo README at
`4e50cd96168484d718667260266df277459ded22` has the heading `## Install`, so its
anchor is `#install`. The page opens, but it does not jump to the section.

Correct the constant, the guide and the assertions in
`apps/web/src/features/todo/MicrosoftTodoAction.test.tsx` (L43 and L101) together.

## Related

[Title-length policy for linked tasks](microsoft-linked-title-length-policy.md):
do not silently truncate remote titles and send the truncated value back during
a later merge.
