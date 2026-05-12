---
title: HTTP Bridge
description: Every endpoint the daemon exposes over HTTP, with auth, request/response shapes, and curl recipes.
---

The HTTP bridge runs alongside the daemon when `[bridge] enabled = true`
in `mxr.toml`. It exposes the same IPC contract the TUI uses, but over
HTTP — so desktop apps, mobile clients, agent runners, and your own
shell scripts all talk to the same daemon through one stable surface.

The bridge serves an OpenAPI 3.1 spec at
`http://mxr.localhost:42829/api/v1/openapi.json` (port and host configurable
in `[bridge]`). The desktop app generates its TypeScript client from
this spec — you can do the same for any language with `openapi-generator`
or `openapi-typescript`.

:::tip[curl-friendly auth]
Get the auth token from `~/.config/mxr/bridge-token` (or wherever
`[bridge] token_path` points). All examples below assume:

```bash
export MXR_TOKEN=$(cat ~/.config/mxr/bridge-token)
# Discover the actual port (custom ports and --auto-port write the bound
# port to <config_dir>/bridge-port).
export MXR_PORT=$(cat ~/Library/Application\ Support/mxr/bridge-port 2>/dev/null \
                   || cat ~/.config/mxr/bridge-port 2>/dev/null \
                   || echo 42829)
export MXR_BASE=http://mxr.localhost:$MXR_PORT
```
:::

## Auth

Every request needs `Authorization: Bearer $MXR_TOKEN`. WebSocket
clients can also pass the token via the `Sec-WebSocket-Protocol`
subprotocol or as a `?token=` query string.

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
- The connecting peer is **not** a loopback address — the bridge is bound to a non-loopback interface and the caller is on a different machine.

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
  "accounts": ["me@example.com"],
  "total_messages": 12044,
  "sync_statuses": [...]
}
```

## Top-level endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/v1/health` | Unauthenticated liveness probe |
| `GET` | `/api/v1/openapi.json` | OpenAPI 3.1 spec |
| `GET` | `/api/v1/docs` | Swagger UI |
| `GET` | `/api/v1/events` | WebSocket — daemon events stream |
| `GET` | `/api/v1/desktop/shell` | Desktop manifest (sidebar + commands) |

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
| `GET` | `/mail/snoozed` | List snoozed messages |
| `GET` | `/mail/count` | Count messages matching a query |
| `GET` | `/mail/sync/status` | Per-account sync state |
| `POST` | `/mail/export-search` | Export all threads matching a search |

```bash
curl -G -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/mail/search" \
  --data-urlencode 'q=is:unread from:billing' \
  --data-urlencode 'mode=lexical' \
  --data-urlencode 'limit=20'
```

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
| `POST` | `/mail/messages/{message_id}/flags` | Set message flags (bitmask in body) |

```bash
curl -X POST -H "Authorization: Bearer $MXR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"message_ids":["..."], "starred":true}' \
  "$MXR_BASE/api/v1/mail/mutations/star"
```

## `/api/v1/mail/*` — Productivity surfaces

The "delight features" land here. Each maps 1-1 to its CLI/TUI counterpart.

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
| `POST` | `/mail/scheduled-sends` | `{draft_id, send_at}` |
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

The response is `SenderProfile { profile }`. When present, `profile`
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
| `POST` | `/mail/humanizer/score` | Score text against your voice profile |
| `POST` | `/mail/humanizer/rewrite` | Rewrite toward human-like / on-voice output |

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
| `POST` | `/mail/drafts/{draft_id}/reset-orphan` | Recover an orphaned send |
| `POST` | `/mail/drafts/{draft_id}/send-stored` | Send a stored draft by id |
| `DELETE` | `/mail/drafts/{draft_id}/stored` | Delete stored draft |

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
| `POST` | `/mail/compose/session/update` | Save current edits |
| `POST` | `/mail/compose/session/send` | Send (calls provider) |
| `POST` | `/mail/compose/session/save` | Save to drafts table only |
| `POST` | `/mail/compose/session/discard` | Throw away |

### Attachments and links

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mail/attachments/open` | Materialise attachment to a tempfile |
| `POST` | `/mail/attachments/download` | Stream the attachment |

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
| `POST` | `/platform/accounts/repair` | Repair keychain / stored credentials for a config |
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
| `GET` | `/platform/llm/status` | Runtime LLM provider + model status |

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
`apps/desktop/src/shared/api.generated.ts` under `DaemonEvent`. Common
ones:

- `MutationCompleted` — your last mutation landed (or rolled back)
- `SyncStarted` / `SyncFinished`
- `ReminderTriggered` — auto-reminder fired
- `ScheduledSendFlushed` — a `Send Later` draft just went out
- `IndexBootstrapped` — Tantivy completed a startup repair

```bash
# Quick subscribe via websocat
websocat -H "Authorization: Bearer $MXR_TOKEN" \
  ws://mxr.localhost:$MXR_PORT/api/v1/events
```

## Generating a typed client

```bash
# TypeScript
npx openapi-typescript $MXR_BASE/api/v1/openapi.json -o src/api.generated.ts

# Python
openapi-generator generate -i $MXR_BASE/api/v1/openapi.json -g python -o ./mxr-py

# Rust
openapi-generator generate -i $MXR_BASE/api/v1/openapi.json -g rust -o ./mxr-rs
```

## See also

- [CLI reference](/reference/cli/) — same surface, terminal-friendly.
- [Recipes](/guides/recipes/) — composing the bridge with curl/jq/agents.
- [Desktop app](/guides/desktop-app/) — first-party consumer of this bridge.
- [For agents](/guides/for-agents/) — boundaries when an LLM drives the API.
- Contributors: see `docs/guides/http-bridge.md` in the repo for the internal architecture and security model.
