# Scoped agent profiles let unlisted requests reach every account

**Status:** fixed on `fix/agent-scope` · **Found:** 2026-10-07 on
`origin/main` 5a15b001 (v0.6.55+) · **Severity:** high for anyone relying on
`allowed_accounts` to keep an MCP client off an account

## What was exposed

`[agents.profiles.mcp]` and `[agents.profiles.agent]` take an
`allowed_accounts` list. The daemon enforced it in
`enforce_account_allowlist` (`crates/daemon/src/handler/mod.rs`), which
asked `request_account_scope` what a request touched. That function checked
a hand-kept list of requests and ended in `_ => Ok(RequestAccountScope::None)`,
and `None` meant "touches no account": the request ran unchecked. Any
request missing from the list failed open.

Two shapes slipped through:

- Requests that carry an `account_id` but weren't in `request_account_id`.
  The list only denied their `account_id: None` form, so naming another
  account's id explicitly passed. This covered `ListRecords`,
  `AnswerFromRecords` and `ExportRecords` (all exposed as MCP tools), plus
  `TriageSearch`, `UnsubscribePurge`, `ExportSearch`, `GetSenderProfile`,
  the screener requests, `ListInvites`, `CreateLabel`/`DeleteLabel`/
  `RenameLabel`, `Wrapped`, `ListStaleThreads`, the contact analytics,
  `CreateSavedSearch`/`RunSavedSearch`, `DraftEval` and the signature-default
  requests. Their `None` forms (every account) also passed for most of these.
- Requests addressed by entity id that the list never mentioned:
  `GetThreadContext`, `GetThreadGist`, `GetThreadGists` (MCP tools),
  `GetThreadBriefing`, `SetFlags`, `SetAutoReminder`/`CancelAutoReminder`,
  `GetDelivery`/`ResolveDelivery`/`DismissDelivery`, `ResolveCommitment`,
  `GetDecision`, `UndoMutation`, `GetJob` and `ResetOrphanedDraft`.
  `SetModeDone` ignored its `todo_ids`, `GetPerson` ignored its `topic`
  thread, `ResolveDeskItems` ignored its `commitment_id`, and
  `UpdateDraft`/`SaveDraft` trusted the draft body's `account_id` without
  checking the stored draft they overwrite. `ListEnvelopes` with your own
  `account_id` and another account's `label_id` listed that label's mail,
  because `list_envelopes` reads by label and ignores the account.

Daemon-wide requests (rules, logs and events, the activity log, saved
searches, semantic and analytics rebuilds, LLM config, `Shutdown`) were
also unlisted and so allowed.

The Archive entity requests from #294 (`GetRecord`, `SetRecordField`,
`DismissRecord`, `FileRecord`, `SetRecordSender`) and the To do mutations
were already resolved to their accounts; the original report that they were
missing was wrong for those. The account-named forms were the gap.

## Who could reach it

Only clients that tag IPC as `mcp` or `agent` are profiled. The CLI always
tags `cli` and the TUI and web their own kinds, so they are never scoped.
In practice that means the model behind `mxr mcp serve`: with
`allowed_accounts = ["work"]` it could still call `mxr_records_list`,
`mxr_records_ask` or `mxr_records_export` with the personal account's id
and get that account's receipts, invoices and totals, and call
`mxr_thread_context` / `mxr_thread_gists` on any thread id to read its ask,
gist and promises. The profile's safety policy and send/destructive gates
still applied, so a read-only profile could read across accounts but not
change them; a `full` profile could also mutate through the unlisted
mutating requests.

The source kind is self-declared by the client, so the profile is a
guardrail for cooperative clients like the MCP server, not a boundary
against a hostile local process.

## Since when

- c0651b54 (v0.5.59, 2026-06-04) added the profiles with the `_ => None`
  fallback; every request listed above that already existed was open from
  then.
- e705d444 (#238, v0.6.35) added `GetThreadContext` and `GetThreadGist`,
  both MCP tools.
- 4e4b0615 (#255, v0.6.42) added `GetThreadGists`, an MCP tool.
- 584ed33b (#290, v0.6.54) added `ListMessages` and `GetPerson`; the
  `topic` thread was unchecked.
- 260a2991 (#294, v0.6.55) added `ListRecords`, `AnswerFromRecords` and
  `ExportRecords` as MCP tools with only their `None` forms denied.

## Fix

`crates/daemon/src/handler/account_scope.rs` replaces the list with
`request_scope`, an exhaustive match with no catch-all arm that returns
`Unscoped`, `AllAccounts` or `Targets(...)`. A new `Request` variant does
not compile until it is classified. Targets name an account, an account
key, a label that must belong to the named account, or an entity (message, thread, draft, draft body, to-do, record,
delivery, commitment, decision, undo entry, job, auth session); each entity
is resolved to its account from the store before the request runs, and an
id that resolves to nothing is denied. `AllAccounts` is denied for a scoped
profile.

`Unscoped` is kept to requests with no account data: `Ping`,
`Authenticate`, `GetStatus` (the version handshake every client and the MCP
status tool use; its account rows are cut to the allowed accounts, see
below), LLM and semantic status reads, notification chimes,
`ListSignatures` (filtered, see below), the humanizer, `ResolveTime` and
the mode guides.

Behaviour change for scoped profiles: rules, logs, the activity log, saved
searches, jobs listing and daemon maintenance are now denied, and `GetJob`
is denied until the job reports which accounts it touched.

Tests: `crates/daemon/src/handler/tests/account_scope.rs` checks each
request family (Archive, To do, modes, Messages, threads/desk/places and the
previously unlisted requests) for own-account allowed and other-account
denied, entity ids owned by the other account, mixed batches, and dispatch
through `handle_request`.

## Left open

- `allowed_accounts` matches by account email and config key as well as id,
  so two accounts sharing an email are allowed together.

## Review of 0421b70f: more leaks

A review of the first fix found paths it didn't close. Each is fixed
in its own commit with a test that failed first.

- **Shared thread ids (high).** Legacy Gmail thread ids aren't
  account-scoped, so one id can hold two accounts' messages. Resolution used
  `get_threads_batch`, which reports one account per id, while thread loads
  return every account's messages. Threads now resolve through
  `Store::thread_account_pairs` to every account holding them, and the
  request is denied if any is outside the scope. As a second net,
  `scope_response` cuts a `Thread` response to the allowed accounts'
  messages, rebuilds its counts, participants and snippet from them, and
  drops a cached summary written over the hidden ones.
- **Status (medium).** `GetStatus` stays allowed as the handshake, but its
  response is cut to the allowed accounts' names, sync statuses and message
  count. Daemon-level fields (versions, pid, uptime, feature health,
  semantic runtime) are unchanged.
- **Signatures and snippets (medium).** `ListSignatures` hides any signature
  that a default binds to an excluded account; unbound signatures and the
  client's own stay. Snippets have no account binding at all, so the snippet
  requests are classified `AllAccounts` and denied to scoped profiles. That
  trades away snippet use for scoped agents rather than show them text
  written for any account.
