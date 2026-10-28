---
title: HTTP Bridge
description: Every endpoint the daemon exposes over HTTP, with auth, request/response shapes, and curl recipes.
---

The local HTTP bridge runs alongside the daemon when `[bridge] enabled = true`
in the active `config.toml`. The web app and local scripts use its HTTP routes
over the daemon's IPC contract. For another computer, use the separate
[remote mailbox gateway](/guides/remote-mailbox/) and pair that client.

The remote gateway exposes pairing, exact-client cleanup and authenticated WSS
mailbox reads. It has no local-token bootstrap, filesystem or administration
routes. The endpoints below describe the local bridge; its token-bootstrap
workflow is not a remote hosting setup.

The bridge serves an OpenAPI 3.1 spec at
`http://mxr.localhost:42829/api/v1/openapi.json` (port and host configurable
in `[bridge]`). The spec is authenticated like the rest of the bridge API. It
covers most routes and the shared protocol types; a few live routes are still
missing. The web app uses `openapi-typescript` over the spec, then
`openapi-fetch` on top. Use the route tables below for operational detail.

The [API route inventory](/reference/api-explorer/) reads the website's
[`/openapi.json`](/openapi.json) download. OpenAPI CI compares that checked-in
export with `mxr-web` without refreshing the input first. A daemon's
`/api/v1/openapi.json` describes its installed version, which may differ from
the website. Freshness does not mean complete contracts: some declared routes
still lack request/response schemas, and some live routes are not declared.

:::tip[curl-friendly auth]
Get the auth token from the active profile config directory's
`bridge-token` file, unless `[bridge].token_path` or
`MXR_BRIDGE_TOKEN_PATH` points somewhere else. All examples below assume:

```bash
export MXR_CONFIG="$(mxr config path)"
export MXR_TOKEN="$(cat "$(dirname "$MXR_CONFIG")/bridge-token")"
# Discover the actual port (custom ports and --auto-port write the bound
# port to <config_dir>/bridge-port).
export MXR_PORT="$(cat "$(dirname "$MXR_CONFIG")/bridge-port" 2>/dev/null \
                   || echo 42829)"
export MXR_BASE="http://mxr.localhost:$MXR_PORT"
```
:::

## Auth

Every request except `/api/v1/health`, `/api/v1/auth/local-token`, and
`/api/v1/i18n` needs `Authorization: Bearer $MXR_TOKEN`. OpenAPI and
Swagger UI are not special-cased. WebSocket clients can also pass the token via the
`Sec-WebSocket-Protocol` subprotocol or as a `?token=` query string.

A plain browser location bar cannot attach an `Authorization` header. For
generated clients, dump `/api/v1/openapi.json` with `curl` first. For normal
browser use, open the first-party web app with `mxr web`.

### Same-machine auto-handshake

The SPA served by `mxr web` doesn't ask the user to paste a token.
`GET /api/v1/auth/local-token` is an unauthenticated endpoint that
returns the bridge token to callers whose TCP peer is a loopback IP.

```bash
curl http://mxr.localhost:$MXR_PORT/api/v1/auth/local-token
# → {"token":"<uuid>","source":"local-handshake"}
```

The endpoint returns **404** (not 401) when:
- `[bridge].auto_local_token = false` — operator opted out.
- The connecting peer is **not** a loopback address — non-loopback bridge serving is reserved for future TLS-backed remote mode.

This lets the local SPA self-authenticate while keeping the same
strict bearer-handshake story for remote callers.

```bash
curl -H "Authorization: Bearer $MXR_TOKEN" "$MXR_BASE/api/v1/admin/status"
```

Response:

```json
{
  "uptime_secs": 1822,
  "daemon_pid": 4242,
  "instance": "mxr",
  "is_demo": false,
  "accounts": ["me@example.com"],
  "total_messages": 12044,
  "sync_statuses": [...]
}
```

## Top-level endpoints

| Method | Path | Auth | Purpose |
|--------|------|------|---------|
| `GET` | `/api/v1/health` | No | Liveness probe |
| `GET` | `/api/v1/auth/local-token` | Loopback peer only | Same-machine token bootstrap |
| `GET` | `/api/v1/i18n` | No | Locale strings for the first-party web app |
| `GET` | `/api/v1/openapi.json` | Yes | OpenAPI 3.1 spec |
| `GET` | `/api/v1/docs` | Yes | Swagger UI |
| `GET` | `/api/v1/events` | Yes | WebSocket — daemon events stream |
| `GET` | `/api/v1/client/shell` | Yes | Client shell data (sidebar + status) |

## `/api/v1/admin/*` — Daemon health and operations

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/admin/status` | Status snapshot (uptime, pid, accounts, sync) |
| `GET` | `/admin/diagnostics` | DoctorReport with findings + remediation |
| `GET` | `/admin/diagnostics/bug-report` | Bundled bug report (Markdown) |
| `GET` | `/admin/events` | Recent daemon events (paged) |
| `GET` | `/admin/logs` | Recent log lines (paged) |
| `POST` | `/admin/ping` | Liveness round-trip |
| `POST` | `/admin/shutdown` | Graceful daemon shutdown |

After a sync attempt, doctor diagnostics classify an observed missing IMAP sync
credential and point to the active config directory's `secrets.toml` or the OS
keychain. The finding does not include the credential reference or secret.
SMTP send failures are not yet aggregated into doctor diagnostics.

## `/api/v1/mail/*` — Reading

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/mailbox` | Browse the inbox or any lens |
| `GET` | `/mail/search` | Run a Tantivy search |
| `GET` | `/mail/threads/{id}` | Full thread payload (messages + bodies) |
| `GET` | `/mail/threads/{id}/export` | Markdown / JSON export |
| `GET` | `/mail/drafts` | List drafts |
| `GET` | `/mail/messages/{message_id}/body` | Fetch one message body |
| `GET` | `/mail/messages/{message_id}/html-images` | HTML-linked image asset list |
| `GET` | `/mail/messages/{message_id}/headers` | Raw RFC 5322 headers |
| `GET` | `/mail/invites` | List detected calendar invites (`?limit=200`) |
| `GET` | `/mail/snoozed` | List snoozed messages |
| `GET` | `/mail/deliveries` | List tracked deliveries (`?filter=active\|delivered\|all\|dismissed`) |
| `GET` | `/mail/deliveries/{id}` | One delivery + its source message ids |
| `GET` | `/mail/count` | Count messages matching a query |
| `GET` | `/mail/sync/status` | Per-account sync state |
| `POST` | `/mail/export-search` | Export all threads matching a search |

```bash
curl -G -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/mail/search" \
  --data-urlencode 'q=is:unread from:billing@example.com' \
  --data-urlencode 'mode=lexical' \
  --data-urlencode 'limit=20'
```

When the daemon's search index is unavailable, `/mail/search` answers plain
words, `subject:` and `from:` from SQLite and adds
`degraded: {reason, searched_fields, notice}`. The field is absent on a normal
reply. `/mail/mailbox` carries it as `mailbox.degraded` for saved-search and
subscription lists. Other queries fail with code `search_index_unavailable`.

## `/api/v1/mail/*` — Mutations

All mutations accept `message_ids: string[]` in the JSON body unless
noted. They emit a `MutationCompleted` event over the WebSocket so
clients can reconcile optimistically.

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/mutations/archive` | Remove from inbox |
| `POST` | `/mail/mutations/trash` | Move to trash |
| `POST` | `/mail/mutations/spam` | Mark as spam |
| `POST` | `/mail/mutations/star` | Star / unstar (`{starred: bool}`) |
| `POST` | `/mail/mutations/read` | Mark read / unread (`{read: bool}`) |
| `POST` | `/mail/mutations/read-and-archive` | Combined |
| `POST` | `/mail/mutations/labels` | Modify labels (`{add, remove}`) |
| `POST` | `/mail/mutations/move` | Move to another label |
| `POST` | `/mail/mutations/undo` | Undo via `mutation_id` |
| `POST` | `/mail/sync` | Trigger sync |
| `POST` | `/mail/snoozed/{id}/wake` | Force-unsnooze |
| `GET` | `/mail/actions/snooze/presets` | Available snooze presets |
| `POST` | `/mail/actions/snooze` | Snooze messages |
| `POST` | `/mail/actions/unsubscribe` | Unsubscribe from list mail |
| `POST` | `/mail/actions/invite/reply` | Dry-run or send a calendar invite RSVP |
| `POST` | `/mail/deliveries/scan` | Backfill scan (`{since_days, dry_run}`) |
| `POST` | `/mail/deliveries/{id}/resolve` | Mark a delivery delivered/done |
| `POST` | `/mail/deliveries/{id}/dismiss` | Hide a false positive |
| `POST` | `/mail/messages/{message_id}/flags` | Set message flags (bitmask in body) |

### Retained mailbox Archive operations

These routes implement the provider Inbox action, not filing a record in
[Archive](/guides/archive/) or the desk's **Done** action. Get the workspace
identity first. Preview retains the reviewed selection; submit must reuse that
identity, review ID and mutation. Query is read-only. After a timeout, query
the same identity; submitting it again resumes only `pending` items and never
repeats an `unknown` provider effect.

| Method | Path | Purpose and JSON body |
|--------|------|-----------------------|
| `GET` | `/mail/archive-operations/workspace` | Read `workspace_id` and `recovery_generation` |
| `POST` | `/mail/archive-operations/preview` | `{identity, mutation}`; retain an immutable review |
| `POST` | `/mail/archive-operations/submit` | `{identity, review_id, mutation}`; submit that retained review |
| `POST` | `/mail/archive-operations/query` | `{identity}`; read the retained per-message outcomes |

`identity` contains `operation_id`, `caller_id` and a `workspace` object with
the two values returned by the workspace route. Before submitting, durably
retain the complete preview response, including its identity, review ID and
selection. The server retains the review, but cannot recover the operation ID
for a client that loses its preview response. Use the retained identity for queries, and include its review ID when
submitting. Request and response types are defined by the generated OpenAPI
spec.

```bash
curl -X POST -H "Authorization: Bearer $MXR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"message_ids":["..."], "starred":true}' \
  "$MXR_BASE/api/v1/mail/mutations/star"
```

List detected calendar invites:

```bash
curl -G -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/mail/invites" \
  --data-urlencode 'limit=50'
```

Dry-run calendar replies before sending the iMIP `METHOD:REPLY` email:

```bash
curl -X POST -H "Authorization: Bearer $MXR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"message_id":"MESSAGE_ID","action":"accept","dry_run":true}' \
  "$MXR_BASE/api/v1/mail/actions/invite/reply"
```

## `/api/v1/mail/*` — Productivity surfaces

The "delight features" land here. Each maps 1-1 to its CLI/TUI counterpart.

### Now and the modes

The daemon caps Now's sections and writes every line, so a client only
draws them. Done here previews with `dry_run: true` and returns one
`mutation_id` that `/mail/mutations/undo` reverses.

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/now` | Now: People, Due soon, the Updates card and the evening Reading pick (`?account=`) |
| `GET` | `/mail/arrivals` | Now's arrivals line: every email first seen since Now was last opened, counted once by where it went, with Not-sure questions and the track record (`?account=&mark_seen=`; `mark_seen=true` when Now opens starts a visit) |
| `GET` | `/mail/arrivals/list` | The emails behind one count, newest first (`?bucket=reading&since=&until=&limit=100&account=`); `since` and `until` default to the line's window |
| `POST` | `/mail/arrivals/modes` | Where each of up to 200 emails went and where it is now, for Inbox's chips (`{message_ids}`) |
| `POST` | `/mail/messages/{message_id}/move` | Move one email to a mode, or with `sender` set the sender's mode (`{mode, sender?, dry_run?, source?}`; `source: "not_sure"` answers Now's question); returns the `correction_id` undo takes |
| `POST` | `/mail/moves/{correction_id}/undo` | Put a move back exactly as it was; undoing twice changes nothing |
| `GET` | `/mail/corrections` | Every move, sender mode and Not-sure answer, newest first (`?account=&limit=50`) |
| `GET` | `/mail/people` | Messages defaults to actionable topics only. `?turn=all` explicitly includes history; `mine` and `theirs` select a side's turn. `unread=true`, `pace=true`, `limit=` and `account=` narrow the selected view. `row_count` and `thread_count` describe attention; `matched` counts the selected view's rows |
| `GET` | `/mail/people/page` | A person's page with the selected topic as new text (`?person=&topic=`) |
| `POST` | `/mail/people/ack` | Got it: `{thread_id}` previews (dry run by default) and returns a `preview_token`; a send needs `dry_run: false`, the token (within a minute) and the previewed `expect_text`, and is refused if anything changed or it was already acknowledged |
| `POST` | `/mail/people/merge` | Merge addresses into one person by hand (`dry_run` previews) |
| `POST` | `/mail/people/split` | Take an address back out of its person (`dry_run` previews) |
| `GET` | `/mail/people/merge-suggestions` | Merges mxr suggests: same name, written to both |
| `GET` | `/mail/rail` | Now, the five modes and Inbox with keys, counts and early-version notes (Messages also carries `quiet`), plus More |
| `POST` | `/mail/threads/previews` | Subject, latest sender, date, snippet, message and unread counts and an attachment flag for up to 100 threads, with no bodies (`{thread_ids}`; unknown ids are left out) |
| `GET` | `/mail/modes/membership` | Which modes hold one thread and why (`?thread_id=` or `?message_id=`) |
| `POST` | `/mail/modes/membership` | The same for up to 100 threads (`{thread_ids}`) |
| `POST` | `/mail/modes/{mode}/done` | Done here in `messages`, `todo`, `updates` or `reading` (`{thread_ids, dry_run, todo_ids?, sender?}`): `todo_ids` ticks off only those to-dos, `sender` (`{account_id, sender_email}`) covers all of that sender's threads in Updates or Reading; each outcome carries the toast copy |
| `GET` | `/mail/modes/guide` | How a mode explains itself (`?mode=now`) |
| `POST` | `/mail/hints/{hint}` | Dismiss a hint in every client, or show it again (`{seen}`) |

```bash
curl -X POST -H "Authorization: Bearer $MXR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"thread_ids":["..."], "dry_run":true}' \
  "$MXR_BASE/api/v1/mail/modes/messages/done"
```

### To do

All paths below are relative to `/api/v1`. Responses use the daemon's To do
data. For each mutation, send `dry_run: true` first and review the returned
preview before repeating with `dry_run: false`.

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/mail/todos` | Runway; optional `account` and `mark_seen` |
| `POST` | `/mail/todos` | Create from `{message_id, title, kind?, due?, time_zone?, dry_run}` |
| `GET` | `/mail/todos/in/{state}` | List `open`, `done`, `dismissed` or `expired`; optional `account` and `limit` |
| `GET` | `/mail/todos/{todo_id}` | Task and field provenance |
| `POST` | `/mail/todos/state` | Change state with `{todo_ids, action, dry_run}` |
| `POST` | `/mail/todos/{todo_id}/schedule` | Change visibility with `{when?, time_zone?, dry_run}`; omitted `when` clears the override |
| `POST` | `/mail/todos/{todo_id}/edit` | Correct fields with `{edits, time_zone?, dry_run}` |
| `GET` | `/mail/todos/catchup` | Older candidates awaiting a decision |
| `POST` | `/mail/todos/catchup` | Apply a catch-up decision with a preview |
| `POST` | `/mail/todos/{todo_id}/resolution` | Preview or apply `{suggestion_id, action, dry_run}` |
| `GET` | `/mail/todos/microsoft` | Optional CLI status: `installed`, installation URL and hint |
| `POST` | `/mail/todos/{todo_id}/microsoft` | Preview with `{dry_run:true,account_id?}`; link with `{dry_run:false,fingerprint,account_id?}` using that preview's fingerprint |
| `POST` | `/mail/todos/microsoft/sync` | Reconcile `{todo_id?,account_id?,dry_run}`; omitted ID selects links within the selected/allowed accounts |

Microsoft linking returns `MicrosoftTodoSend` with `result.state` of `preview`,
`linked`, `attention` or `unknown`. A changed preview fingerprint is refused.
Sync returns `MicrosoftTodoSync`; each result has a state of `synced`, `changes`,
`conflict` or `paused`, a summary and, when available, `mixer`, `microsoft` and
`merged` shared-field snapshots. A dry run reads the peer's cache without changing
tasks or link receipts. Linking authorizes ongoing two-way sync only for that
task. See [Link a task with Microsoft To Do](/guides/todo/#link-a-task-with-microsoft-to-do).

For resolution offers, `action` is `preview`, `accept`, `dismiss` or `undo`.
The task ID and suggestion ID must identify the same current offer. Acceptance
can complete the task or revise its date; the quoted source and proposed fields
are available in the preview. See [Review later evidence](/guides/todo-resolution/)
and the generated [API route inventory](/reference/api-explorer/) for schemas.

```bash
curl -H "Authorization: Bearer $MXR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"suggestion_id":"SUGGESTION_ID","action":"accept","dry_run":true}' \
  "$MXR_BASE/api/v1/mail/todos/TODO_ID/resolution"
```

### Archive

Records: receipts, orders, bookings, bills and documents built from mail.
Each route is a passthrough to the daemon request it names. Mutations take
`dry_run: true`, which runs the same write in a transaction that is rolled
back, so the preview is what the real call does.

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/records` | The ledger: a page of records newest first, with month counts and totals, facets and what is coming up (`?account=&kind=order,booking&issuer=&year=&min_amount_minor=&max_amount_minor=&has_pdf=&checked=&group=&limit=200&offset=`) |
| `GET` | `/mail/records/answer` | The answer box (`?q=lisbon booking ref&fallback=true&limit=4`): the field from record data, else `mxr ask`'s answer as `fallback` |
| `GET` | `/mail/records/subscriptions` | Subscriptions worked out from the records: one per issuer and product at a steady cadence, with history, price changes, next charge, status, signals and totals per currency (`?account=`) |
| `GET` | `/mail/records/{record_id}` | One record's card: every field with its provenance, documents and source emails (a full id or a unique prefix) |
| `POST` | `/mail/records/{record_id}/field` | Fix, confirm or clear a field, or confirm the whole card (`{edit: {op: "set", field, value} \| {op: "confirm", field} \| {op: "confirm_all"} \| {op: "clear", field}, apply_to_sender?, dry_run?}`) |
| `POST` | `/mail/records/dismiss` | Not a record, or back with `restore` (`{record_ids, restore?, dry_run?}`); the email is never touched |
| `POST` | `/mail/records/file` | File an email as a record yourself (`{message_id, kind?, dry_run?}`) |
| `POST` | `/mail/records/sender` | Always or never file a sender's mail (`{message_id, verdict: "always" \| "never" \| null, kind?, dry_run?}`) |
| `POST` | `/mail/records/export` | CSV export, and PDFs to an absolute folder (`{account_id?, filter, attachments_dir?, dry_run?}`); the dry run returns the row count, totals, unchecked rows and missing PDFs |

```bash
curl -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/mail/records/answer?q=lisbon%20booking%20ref"
```

### Updates

The daemon builds the digest, its sections and the let-go selection. Pass
the digest's `selection_token` to `/mail/updates/let-go`; the run refuses
if the cut changed since.

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/updates` | The digest at the latest cut (`?account=`, `?cut=` RFC3339, `?mark_seen=true`, `?expired=true`) |
| `POST` | `/mail/updates/let-go` | Let go of the cut, or one source in it (`{account_id?, cut?, source_key?, selection_token?, dry_run}`); returns the line, the threads and the undo `mutation_id` |
| `GET` | `/mail/codes` | One-time codes from the last day's mail, newest first, each current or expired (`?account=`, `?limit=` default 10, at most 50) |
| `POST` | `/mail/updates/sources` | Tune a source (`{account_id?, source, setting, dry_run}`, `setting` one of `every_digest`, `changes_only`, `muted`, `breakthrough`); returns the prior setting |

```bash
curl -X POST -H "Authorization: Bearer $MXR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"dry_run":true}' \
  "$MXR_BASE/api/v1/mail/updates/let-go"
```

### Reading

The daemon cuts newsletters into items, bands and ranks them, and owns the
article fetch's guards. Item keys are `<message id>:<index>`; URL-encode
them in a path. Let go is `/mail/modes/reading/done` and unsubscribe is
`/mail/actions/unsubscribe-purge`, both previewed with `dry_run: true`.
For unsubscribe, commit through `/mail/actions/unsubscribe-preview/commit`
with the dry run's `preview_token` and an explicit `archive` choice. Set
`archive: false` to keep existing mail or `true` to mark read and archive the
previewed issues. Both choices use the method shown in the preview. Older
bridges reject this route; upgrade the bridge and daemon before retrying.
The existing `/mail/actions/unsubscribe-purge` commit retains its clear-mail behaviour.

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/reading` | The edition: bands, Later, sources (`?account=`, `?mark_visit=true` when the user opens Reading) |
| `GET` | `/mail/reading/items/{item_key}` | One item for the reader: text, cleaned HTML, saved article, highlights, minutes left. Never fetches |
| `POST` | `/mail/reading/later` | Put items on Later or take them off (`{item_keys, later, dry_run}`) |
| `POST` | `/mail/reading/engagement` | Local engagement (`{item_key, opened, dwell_ms, progress}`); not written with `MXR_ACTIVITY=off` |
| `POST` | `/mail/reading/items/{item_key}/article` | Fetch and save the linked article, contacting its site (`{refresh}`) |
| `GET` | `/mail/reading/highlights` | Every highlight, plus the same as Markdown |
| `POST` | `/mail/reading/highlights` | Save a passage (`{item_key, quote, note?, view?, paragraph?}`); `paragraph` is the 0-based paragraph it was selected in, so it marks that passage and not the first match |
| `POST` | `/mail/reading/highlights/{id}/note` | Replace a highlight's note (`{note, dry_run?}`). `note` is required: blank clears it, `null` is refused. Returns a `mutation_id` for `POST /mail/mutations/undo` |
| `POST` | `/mail/reading/highlights/{id}/remove` | Remove a highlight, or with `{dry_run: true}` name what it would remove. Returns a `mutation_id` for undo |
| `POST` | `/mail/reading/sources` | Per-source settings (`{account_id, sender_email, original_layout?, dismiss_unsubscribe_offer?}`) |
| `GET` | `/mail/reading/marginalia` | Whether [Marginalia](/guides/marginalia/) is connected, its address and how many items were sent |
| `POST` | `/mail/reading/marginalia` | Connect (`{connection: {url, token}}`, the token checked with Marginalia first) or disconnect (`{connection: null}`) |
| `POST` | `/mail/reading/items/{item_key}/marginalia` | Send the item to Marginalia, or find it there if sent before (`{dry_run}`); the dry run contacts nothing |
| `POST` | `/mail/reading/marginalia/highlights` | Read back highlights made in Marginalia (`{item_key?}`; omitted reads every sent item) |

### Reply-later queue

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/reply-later` | List flagged messages |
| `POST` | `/mail/reply-later/{message_id}` | Set/clear flag (`{flag: bool}`) |

### Auto-reminders

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/reminders` | Schedule (`{sent_message_id, remind_at}`) |
| `DELETE` | `/mail/reminders/{message_id}` | Cancel |

### Send Later

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/scheduled-sends` | `{draft_id, send_at, expected_revision}`; `expected_revision` is required for scheduling |
| `DELETE` | `/mail/scheduled-sends/{draft_id}` | Cancel pending send |

### Snippets

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/snippets` | List |
| `POST` | `/mail/snippets` | Create / update (`{name, body, vars}`) |
| `DELETE` | `/mail/snippets/{name}` | Remove |

### Sender view

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/sender?account_id=...&email=...` | Per-sender aggregates plus recent messages from that sender |
| `GET` | `/mail/contacts/autocomplete?q=...&limit=...` | Suggest recipients from known inbound senders: `{contacts: [{name, email}]}` for senders whose address or display name contains `q` (case-insensitive), one entry per address, `name` null when no row has one. `limit` defaults to 10 |

The sender response is `SenderProfile { profile }`. When present, `profile`
includes `recent_messages`: the newest messages from that sender with
`message_id`, `thread_id`, `subject`, `snippet`, `date`, `direction`,
and an attachment-present flag. Clients use this to render "Other
emails from sender" and deep-link directly into the matching thread.

### Screener

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/screener/queue?account_id=...&limit=...` | Senders awaiting decision |
| `GET` | `/mail/screener/decisions?account_id=...` | All existing decisions |
| `POST` | `/mail/screener/decisions` | `{account_id, sender_email, disposition, route_label?}` |
| `DELETE` | `/mail/screener/decisions` | Clear (`{account_id, sender_email}`) |

`disposition` is one of: `allow`, `deny`, `feed`, `paper_trail`, `unknown`.

### LLM features

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/threads/{thread_id}/summarize` | Concise Markdown thread summary + next steps |
| `POST` | `/mail/threads/draft-assist` | `{thread_id, instruction}` → suggested reply body plus model/humanizer/voice metadata |
| `POST` | `/mail/drafts/new` | Start an LLM-backed draft from a prompt |
| `POST` | `/mail/drafts/refine` | Refine draft body with model knobs |
| `POST` | `/mail/compose/assist` | `{draft, text, selection?, action?, instruction?, adjust?, revise?}` → `{proposal}`: writing help for the text in a compose box. `draft` is `{kind: "stored", draft_id}`, `{kind: "reply", message_id, all}`, `{kind: "forward", message_id, to}` or `{kind: "new", account_id, to, subject}`. `revise` is the id of the proposal on screen: the model is shown it and the instruction is applied to it, while `text` stays the user's own text. Changes nothing |
| `POST` | `/mail/compose/apply` | `{proposal_id, draft_id}` → `{draft_id, text_hash}`: write a proposal into a stored draft's text region; 422 if that text changed since the proposal |
| `POST` | `/mail/humanizer/score` | Score text against your voice profile |
| `POST` | `/mail/humanizer/rewrite` | Rewrite toward human-like / on-voice output |

### Assistant

The [assistant](/guides/assistant/) runs in the daemon. Sending and
deciding return as soon as the turn starts; follow `AssistantUpdated` on
the events stream and refetch the conversation.

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/assistant/availability` | Whether it can run, on which model, and whether conversations leave the machine |
| `GET` | `/mail/assistant/models` | The models the assistant can be set to, which can be picked now and why not, and the one in use |
| `POST` | `/mail/assistant/model` | Set the model for every conversation from the next message; refused for a model outside the list, without its key, or off this machine while the cloud setting is off |
| `GET` | `/mail/assistant/conversations` | Conversations, most recently active first (`?limit=`) |
| `POST` | `/mail/assistant/conversations` | `{conversation_id?, message, context}`: send a message, starting a conversation without an id |
| `GET` | `/mail/assistant/conversations/{id}` | The transcript and anything waiting for you |
| `GET` | `/mail/assistant/conversations/{id}/steps/{call_id}/rows` | Every row one step's stored result lists, up to 500 (`?seq=` is the seq of the assistant message that declared the call) |
| `POST` | `/mail/assistant/conversations/{id}/decide` | `{decisions: [{call_id, decision: approve \| reject \| answer, ...}]}`; approving runs exactly the stored call |
| `POST` | `/mail/assistant/conversations/{id}/cancel` | Stop a running turn, or close what is waiting without running it |
| `POST` | `/mail/assistant/conversations/{id}/auto-approve` | `{enabled}`: let reversible changes run without asking in this conversation |
| `POST` | `/mail/assistant/conversations/{id}/delete` | `{dry_run}`: delete the conversation and its transcript |

### Relationship and commitments

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/relationship?account_id=...&email=...` | Per-contact relationship profile |
| `POST` | `/mail/relationship/rebuild` | JSON `{account_id, email}` — rebuild relationship summaries |
| `GET` | `/mail/commitments?account_id=...&email=...&status=...` | List open commitments |
| `POST` | `/mail/commitments/{commitment_id}/resolve` | Mark a commitment resolved |

### Stored drafts (local IPC parity)

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/drafts/orphaned` | Mid-send / stuck drafts |
| `POST` | `/mail/drafts/save-local` | Persist a draft row without compose session |
| `POST` | `/mail/drafts/{draft_id}/reset-orphan` | Legacy orphan check; cannot prove rejection or clear a retained send outcome |
| `POST` | `/mail/drafts/{draft_id}/send-stored?expected_revision=REVISION&operation_id=OPERATION_ID` | Send a stored draft with its reviewed revision and operation ID |
| `GET` | `/mail/send-outcomes/{operation_id}?account_id=ACCOUNT_ID` | Query a retained send outcome |
| `POST` | `/mail/send-outcomes/{operation_id}/recover?account_id=ACCOUNT_ID` | Preview legacy cooldown evidence with `{dry_run:true}`; confirm the same evidence using `{dry_run:false,expected_evidence_sha256:"..."}` |
| `DELETE` | `/mail/drafts/{draft_id}/stored` | Delete stored draft and its linked provider draft |

The stored-send query parameters `expected_revision` and `operation_id` are
required. A schedule body also needs the revision returned by the saved-draft
preview. Query outcomes with the owning `account_id`; after scheduling, use the
draft alias `draft:DRAFT_ID` when the schedule operation ID is not known. The
legacy orphan endpoint does not reset or erase accepted or unknown outcome
evidence; a draft still marked Sending is refused because elapsed time cannot
prove non-acceptance.

Outcomes include `waiting`, `accepted`, `not_accepted` and `unknown`. Optional
`recovery` contains `phase`, `attempts`, `retry_at` and `reason`. A waiting send
retries automatically with its retained identity; an unknown send is only
reconciled against positive provider evidence. The recovery POST requires the
original workspace binding and never treats a missing Sent search result as
permission to resend. See [crash-safe drafts](/guides/crash-safe-drafts/) for
the preview/confirmation flow and its limits.

### Signatures

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/mail/signatures` | List |
| `POST` | `/mail/signatures` | Create or update |
| `DELETE` | `/mail/signatures/{name}` | Remove |
| `GET` | `/mail/signature-defaults` | Defaults per context |
| `POST` | `/mail/signatures/default` | Set default |
| `POST` | `/mail/signatures/default/clear` | Clear default |
| `POST` | `/mail/signatures/resolve` | Resolve signature for compose context |

```bash
curl -X POST -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/mail/threads/THREAD_ID/summarize"
```

```json
{
  "kind": "ThreadSummary",
  "text": "Alice asked Bob to confirm the launch checklist; he hasn't replied since Monday.",
  "model": "qwen2.5:3b-instruct"
}
```

### Compose session

The desktop and any other interactive client open a *compose session*
that the daemon owns. The state (frontmatter + body + attachments)
lives server-side until you send/save/discard.

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/compose/session` | Open new / reply / forward |
| `POST` | `/mail/compose/session/refresh` | Refetch latest state |
| `POST` | `/mail/compose/session/restore` | Resume saved draft |
| `POST` | `/mail/compose/session/update` | Save current edits locally and to a linked provider draft |
| `POST` | `/mail/compose/session/send` | Send (calls provider) |
| `POST` | `/mail/compose/session/save` | Save locally or create/update a linked provider draft |
| `POST` | `/mail/compose/session/attachment` | Declare (`begin`), write a bounded chunk (`chunk`), or finalize an immutable draft resource (`finalize`) |
| `POST` | `/mail/compose/session/discard` | Throw away |

For compose uploads, each request names `draft_path`, `account_id` and
`operation`. `begin` supplies resource `metadata` (ID, owner draft/account,
filename, MIME type, exact length and SHA-256). `chunk` supplies `resource_id`,
`offset` and `content_base64` for at most 256 KiB raw bytes. `finalize` supplies
`resource_id` and `expected_revision`; it verifies bytes and CAS-attaches the
resource. Begin returns `draft_id` and the owner `revision`; chunk returns
`{ "ok": true }`. Its response returns `draft_id`, the new `revision`, original
`filename`, `size_bytes` and an `mxr-resource:` reference for compose frontmatter.
Resources are capped at 25 MiB by local policy. A changed compose owner or stale
revision fails instead of attaching to another draft.

#### Bind private compose requests to their workspace

Before opening private compose work, read `/mail/archive-operations/workspace`
and retain its `workspace` object (workspace ID plus recovery generation) with
the local draft snapshot. Send that exact JSON in the `X-Mxr-Workspace` header
on requests for the snapshot. The bridge checks the owner before consuming the
request body and checks compose sidecar ownership before reading or removing
private files. A request without an owner fails with
`workspace_owner_required`; a mismatched workspace or recovery generation
returns `workspace_conflict` without applying the request. The response echoes
the observed owner in `X-Mxr-Workspace`, including on conflicts. Keep the
original owner with the private draft; a reconnect does not make the currently
selected workspace its new owner.


Generic stored-draft send (`/mail/drafts/{draft_id}/send-stored`) and schedule
(`/mail/scheduled-sends`) requests require `managed_attachments_reviewed=true`
when the saved draft contains managed resources. Set it only after reviewing
the current metadata returned by the draft endpoint. Omission defaults to
false; old requests still work for drafts without managed resources. Compose
session sends derive review from the resource entries in their compose file.

### Attachments and links

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/attachments/open` | Open the daemon-local attachment and return its path (legacy local-client contract) |
| `POST` | `/mail/attachments/download` | Materialise the daemon-local attachment and return its path (legacy local-client contract) |
| `POST` | `/mail/attachments/transfer/metadata` | Get path-free attachment identity, filename, MIME type, exact length, SHA-256 and maximum chunk size |
| `POST` | `/mail/attachments/transfer/chunk` | Read a bounded base64 chunk by byte offset; the response echoes message ID, attachment ID and offset |

The transfer routes require the bridge token on every request. Clients verify
the complete length and SHA-256 after assembling chunks. The original open and
download routes remain available for local clients that expect daemon-path
responses; they do not provide client-local files to a remote caller.

### Labels

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/labels/create` | `{name}` |
| `POST` | `/mail/labels/rename` | `{from, to}` |
| `POST` | `/mail/labels/delete` | `{name}` |

## `/api/v1/platform/*` — Rules, accounts, LLM, semantic, analytics

These are the "platform" features — saved searches, rules, account
management, analytics, LLM status, semantic search. Available even
without an active inbox.

### Rules

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/rules` | List |
| `GET` | `/platform/rules/detail?id=...` | Full detail |
| `GET` | `/platform/rules/form?id=...` | Editor-friendly form payload |
| `GET` | `/platform/rules/history?id=...` | Change history |
| `GET` | `/platform/rules/dry-run?id=...&since=...` | Preview matches |
| `POST` | `/platform/rules/upsert` | Create / update |
| `POST` | `/platform/rules/upsert-form` | From the form payload |
| `POST` | `/platform/rules/delete` | `{id}` |

### Saved searches

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/saved-searches` | List |
| `POST` | `/platform/saved-searches/create` | `{name, query, mode}` |
| `POST` | `/platform/saved-searches/update` | `{name, new_name?, query?, search_mode?, sort?, icon?, position?}` — patch by current name. `icon` doubles as a CSS color tag; `position < 0` pins. |
| `POST` | `/platform/saved-searches/delete` | `{name}` |
| `POST` | `/platform/saved-searches/run` | `{name}` |

### Accounts

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/accounts` | Runtime inventory (with health) |
| `GET` | `/platform/accounts/config` | Config-backed account list |
| `POST` | `/platform/accounts/test` | Test credentials |
| `POST` | `/platform/accounts/upsert` | Add / update an account |
| `POST` | `/platform/accounts/authorize` | `{account, reauthorize}` — `AuthorizeAccountConfig` (OAuth / credential handoff) |
| `POST` | `/platform/accounts/repair` | Repair stored IMAP/SMTP passwords for a config |
| `POST` | `/platform/accounts/default` | `{key}` set default |
| `DELETE` | `/platform/accounts/{key}` | Remove |
| `POST` | `/platform/accounts/{key}/disable` | Soft-disable |
| `GET` | `/platform/accounts/{id}/addresses` | Aliases for an account |
| `POST` | `/platform/accounts/{id}/addresses` | Add alias |
| `POST` | `/platform/accounts/{id}/addresses/remove` | Remove alias |
| `POST` | `/platform/accounts/{id}/addresses/primary` | Set primary alias |

### OAuth sessions

The daemon owns OAuth flows so the renderer never sees a refresh token.

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/platform/auth/sessions/start` | Begin an OAuth flow |
| `GET` | `/platform/auth/sessions/{id}` | Poll progress |
| `POST` | `/platform/auth/sessions/{id}/cancel` | Abort |
| `POST` | `/platform/auth/sessions/{id}/complete` | Wrap up after callback |

### LLM

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/llm/config` | Current `[llm]` config, without secrets |
| `POST` | `/platform/llm/config` | Update `[llm]` config and reload provider |
| `GET` | `/platform/keymap` | Every key in every client with `keys.toml` applied, the single-key switch and what in the file is ignored (`Keymap`) |
| `POST` | `/platform/keymap` | Change `keys.toml`: `{change: {kind: "set_action", id, keys} \| {kind: "reset_action", id} \| {kind: "single_keys", on}, dry_run}`; nothing is written on a dry run or a conflict (`KeysUpdate`) |
| `GET` | `/platform/notifications/chimes` | The shared chime setting (`NotificationChimes`) |
| `POST` | `/platform/notifications/chimes` | Change fields of the chime setting: the body is a `NotificationChimesPatchData` (only the fields to change), applied by the daemon to the current setting |
| `GET` | `/platform/llm/status` | Runtime provider + model status and process-local typed outcomes by feature route; status does not probe the endpoint |

### Semantic

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/semantic/status` | Index health + active profile |
| `POST` | `/platform/semantic/enable` | Activate |
| `POST` | `/platform/semantic/reindex` | Rebuild |
| `POST` | `/platform/semantic/backfill` | Backfill chunks / embeddings workload |
| `POST` | `/platform/semantic/profiles/install` | `{profile}` (e.g. `bge-small-en-v1.5`) |
| `POST` | `/platform/semantic/profiles/use` | Switch active profile |

### Voice profile (drafting)

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/voice` | Cached user voice profile used by humanizer / draft assist |
| `POST` | `/platform/voice/rebuild` | Recompute voice profile from sent mail |

### Analytics

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/analytics/wrapped` | Year-in-review |
| `GET` | `/platform/analytics/storage-breakdown` | Disk by sender/mimetype/label |
| `GET` | `/platform/analytics/largest-messages` | Heaviest messages |
| `GET` | `/platform/analytics/stale-threads` | "Whose turn is it?" |
| `GET` | `/platform/analytics/contact-asymmetry` | Reply-imbalance ranking |
| `GET` | `/platform/analytics/contact-decay` | Going-cold relationships |
| `GET` | `/platform/analytics/response-time` | Reply-latency percentiles |
| `POST` | `/platform/analytics/refresh-contacts` | Materialise contacts table |
| `POST` | `/platform/analytics/rebuild` | Rebuild analytics views |

### Subscriptions

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/platform/subscriptions` | Newsletter inventory + ROI |

## Events stream

Connect a WebSocket to `/api/v1/events` and you'll receive a JSON line
per daemon event. The TypeScript shapes are in
`apps/web/src/api/generated.ts` under `DaemonEvent`. Common ones:

- `MutationCompleted` — your last mutation landed (or rolled back)
- `SyncStarted` / `SyncFinished`
- `ReminderTriggered` — auto-reminder fired
- `ScheduledSendFlushed` — a `Send Later` draft just went out
- `IndexBootstrapped` — Tantivy completed a startup repair
- `AccountAddressesChanged` — an account's owned addresses changed (an alias was added or removed, or the primary moved); drop anything cached from the old set
- `AssistantUpdated` — an assistant conversation changed; `progress` says
  what a running turn is doing (`thinking`, `calling`, `writing` with the
  reply so far, `summarizing`)

```bash
# Quick subscribe via websocat
websocat -H "Authorization: Bearer $MXR_TOKEN" \
  ws://mxr.localhost:$MXR_PORT/api/v1/events
```

## Generating a typed client

```bash
curl -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/openapi.json" > spec.json

# TypeScript
npx openapi-typescript spec.json -o src/api.generated.ts

# Python
openapi-generator generate -i spec.json -g python -o ./mxr-py

# Rust
openapi-generator generate -i spec.json -g rust -o ./mxr-rs
```

## See also

- [CLI reference](/reference/cli/) — same surface, terminal-friendly.
- [Recipes](/guides/recipes/) — composing the bridge with curl/jq/agents.
- [Web app](/guides/web-app/) — first-party browser client for this bridge.
- [For agents](/guides/for-agents/) — boundaries when an LLM drives the API.
- Contributors: see `docs/guides/http-bridge.md` in the repo for the internal architecture and security model.
