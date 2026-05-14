# Appendix — Context JSON Schemas

Per-action JSON shapes stored in `user_activity.context_json`. Companion to [APPENDIX-action-catalog.md](./APPENDIX-action-catalog.md).

## Conventions

- Shapes are flat where possible. Nest only when the data demands it (e.g. recipient buckets).
- All timestamps are unix epoch milliseconds (i64).
- Strings inside context are bounded — see [#size-limits](#size-limits).
- `null` and omitted fields are equivalent; consumers must tolerate both.
- Bulk operations encode the affected set as `target_ids: [...]` + `count`, with one activity row representing the whole batch.

## Size limits

Enforced at the recorder before insert. Truncation policy:

| Field | Limit | Behavior |
|---|---|---|
| Subject | 200 chars | Truncate with `…` |
| Draft body prefix | 80 chars | Truncate with `…` |
| Search query | 500 chars | Reject (too-long search is a UI bug) |
| Recipient list | 20 entries | Truncate; encode `truncated_count: N` |
| URL (link.click) | 2000 chars | Truncate with `…` |
| Filename | 255 chars | Truncate |
| Total `context_json` after serialization | 4 KiB | Truncate `target_ids` first, then fall through |

If truncation kicks in, emit `tracing::debug!("activity context truncated")` for visibility.

## Forbidden keys (codified in recorder)

These keys never appear in stored `context_json`:
`password`, `password_hash`, `token`, `access_token`, `refresh_token`, `secret`, `api_key`, `client_secret`, `private_key`, `oauth_token`, `id_token`, `cookie`, `session_id`.

Enforcement: a sanity scrubber walks the JSON at recorder boundary and asserts no forbidden top-level or nested key. Tested via the PII audit in [08-privacy.md](./08-privacy.md#pii-audit-test).

---

## Action-by-action shapes

### `mail.read`, `mail.unread`, `mail.archive`, `mail.unarchive`, `mail.trash`, `mail.untrash`, `mail.star`, `mail.unstar`

Single:
```json
{ "thread_id": "thr_abc123" }
```

Bulk:
```json
{ "thread_id": "thr_abc123", "count": 8, "target_ids": ["thr_abc123", "thr_def456", "..."] }
```
`target_id` column carries the primary; full list is in `context.target_ids`.

### `mail.label`, `mail.unlabel`

```json
{ "thread_id": "thr_abc123", "label": "Finance" }
```

### `mail.move`

```json
{ "thread_id": "thr_abc123", "from": "Inbox", "to": "Archive/Finance" }
```

### `mail.snooze`

```json
{ "thread_id": "thr_abc123", "until": 1715592090123, "preset": "tomorrow_morning" }
```

`preset` is the UX label; `until` is the actual time.

### `mail.unsnooze`

```json
{ "thread_id": "thr_abc123" }
```

### `mail.send`

```json
{
  "draft_id": "draft_ghi789",
  "subject": "Re: Q2 plan",
  "recipients": {
    "to": [{ "email": "bob@example.com", "name": "Bob" }],
    "cc": [],
    "bcc": []
  },
  "has_attachments": false
}
```

### `mail.reply`, `mail.forward`

```json
{ "thread_id": "thr_abc123", "draft_id": "draft_ghi789" }
```

### `mail.unsubscribe`

```json
{ "thread_id": "thr_abc123", "mechanism": "list_unsubscribe_header" }
```

`mechanism` ∈ `list_unsubscribe_header`, `mailto`, `url_click`. URL bodies omitted by default; available in `mechanism=url_click` only if `track_link_clicks=true`.

### `mail.mark_spam`, `mail.unmark_spam`

```json
{ "thread_id": "thr_abc123" }
```

---

### `thread.open`

```json
{ "thread_id": "thr_abc123", "subject": "Q2 plan", "from": "Alice", "message_count": 12 }
```

### `thread.close`

```json
{ "thread_id": "thr_abc123", "duration_ms": 8421 }
```
`duration_ms` is time spent on the thread reader. Optional; emit when known.

### `thread.summarize`

```json
{ "thread_id": "thr_abc123", "model": "local-bge-small", "tokens": 1024, "took_ms": 412 }
```

### `thread.flag_reply_later`, `thread.unflag_reply_later`

```json
{ "thread_id": "thr_abc123" }
```

---

### `draft.create`

```json
{
  "draft_id": "draft_ghi789",
  "recipients": { "to": [{ "email": "bob@example.com" }] },
  "subject": "Re: Q2 plan",
  "in_reply_to": "thr_abc123",
  "body_prefix": "Hi Bob, thanks for the update on…"
}
```

`body_prefix` is the first 80 chars of the draft.

### `draft.update`

```json
{ "draft_id": "draft_ghi789", "fields": ["body", "subject"] }
```

`fields` is the set of fields touched. Bodies are NOT included.

### `draft.discard`, `draft.send`, `draft.save`

```json
{ "draft_id": "draft_ghi789" }
```

### `draft.attach`, `draft.detach`

```json
{ "draft_id": "draft_ghi789", "filename": "report.pdf", "size_bytes": 1048576, "mime": "application/pdf" }
```

---

### `search.run`

```json
{
  "query": "from:alice invoice",
  "mode": "lexical",
  "result_count": 12,
  "took_ms": 23,
  "saved_search_slug": null
}
```

`mode` ∈ `lexical`, `semantic`, `hybrid`. `saved_search_slug` populated when launched from a saved search.

### `search.save`

```json
{ "slug": "alice-invoices", "name": "Alice — invoices", "query": "from:alice invoice" }
```

### `search.delete`, `search.rename`

```json
{ "slug": "alice-invoices", "name": "Alice — invoices" }
```

### `saved.open`

```json
{ "slug": "alice-invoices", "name": "Alice — invoices", "query": "from:alice invoice" }
```

---

### `view.open_mailbox`

```json
{ "mailbox": "inbox" }
```

### `view.open_label`

```json
{ "label": "Finance/2026" }
```

### `view.open_screen`, `view.open_settings`, `view.open_analytics`, `view.open_rules`, `view.open_screener`, `view.open_activity`, `view.open_palette`

```json
{ "screen": "analytics", "section": null }
```

`section` is populated for settings sub-pages.

---

### `account.add`

```json
{ "account_id": "acc_abc", "label": "Personal", "provider": "gmail" }
```

Never includes credentials, OAuth state, or refresh tokens.

### `account.remove`, `account.rename`, `account.signin`, `account.signout`, `account.sync`

```json
{ "account_id": "acc_abc", "label": "Personal" }
```

`account.sync` additionally:
```json
{ "account_id": "acc_abc", "label": "Personal", "synced_messages": 42, "took_ms": 1200 }
```

---

### `rule.create`, `rule.update`, `rule.delete`, `rule.enable`, `rule.disable`

```json
{ "rule_id": "rule_xyz", "name": "Auto-archive newsletters" }
```

### `rule.run`

```json
{ "rule_id": "rule_xyz", "name": "Auto-archive newsletters", "matched": 14, "took_ms": 230 }
```

### `rule.test`

```json
{ "rule_id": "rule_xyz", "name": "Auto-archive newsletters", "would_match": 14 }
```

---

### `snippet.create`, `snippet.update`, `snippet.delete`

```json
{ "snippet_slug": "polite-decline", "name": "Polite decline" }
```

### `snippet.insert`

```json
{ "snippet_slug": "polite-decline", "draft_id": "draft_ghi789" }
```

---

### `link.click` (opt-in)

```json
{
  "url": "https://example.com/some/path",
  "from_message_id": "msg_def456",
  "from_thread_id": "thr_abc123"
}
```

If `track_link_clicks=false`, this action is dropped at the mapper — no row written.

### `attachment.open`

```json
{
  "message_id": "msg_def456",
  "filename": "report.pdf",
  "size_bytes": 1048576,
  "mime": "application/pdf"
}
```

### `attachment.save`

```json
{
  "message_id": "msg_def456",
  "filename": "report.pdf",
  "size_bytes": 1048576,
  "mime": "application/pdf",
  "saved_to": "/Users/alice/Downloads/report.pdf"
}
```

---

### `screener.allow`, `screener.block`, `screener.snooze`

```json
{ "sender_email": "newsletter@example.com", "scope": "sender" }
```

`scope` ∈ `sender`, `domain`.

---

### `reminder.set`

```json
{ "thread_id": "thr_abc123", "when": 1715592090123, "kind": "reply_later" }
```

### `reminder.clear`, `reminder.snooze`

```json
{ "thread_id": "thr_abc123" }
```

---

### `app.start`

```json
{ "client": "tui", "version": "0.7.3", "session_id": "sess_abc" }
```

### `app.stop`

```json
{ "client": "tui", "session_id": "sess_abc", "session_duration_ms": 41021 }
```

---

### `activity.paused`

```json
{ "until": 1715595690123, "reason": "user_command" }
```

`until` may be `null` for indefinite. `reason` ∈ `user_command`, `env_var`.

### `activity.resumed`

```json
{ "auto": false, "reason": "user_command" }
```

`auto` true when the recorder auto-resumed because `paused_until` elapsed.

### `activity.pruned`

```json
{ "tier": "ephemeral", "cutoff": 1712914890123, "deleted": 4823 }
```

### `activity.redacted`

```json
{ "count": 32, "by": "filter", "filter_summary": "since=1d action_prefix=mail." }
```

### `activity.exported`

```json
{ "path": "/tmp/activity-2026-05-13.csv", "format": "csv", "filter_summary": "since=24h", "count": 142, "size_bytes": 24600 }
```

### `activity.cleared`

```json
{ "range": "last_1h", "affected": 47 }
```

`range` ∈ `last_1h`, `last_1d`, `last_7d`, `last_30d`, `all`.

---

## Schema validation

A test in `crates/daemon/tests/activity_context_shapes.rs`:

1. Seeds one row per action via the real mapper.
2. For each row, validates `context_json` against the documented shape for that action.
3. Asserts no forbidden keys (the PII audit in [08-privacy.md](./08-privacy.md)).

Use `jsonschema` or a simple per-action validator function — schema-as-code is fine; no need for a separate JSON-schema file.

## Versioning

If a shape changes:
1. Bump the field non-destructively (additive only, with `serde(default)`).
2. Document the change in this file with a "Since: vN.N" note next to the field.
3. Old rows are unaffected — they simply lack the new field.

Never rename or remove a field — clients may be reading historical rows. If a field must die, mark it deprecated in this doc and stop populating it.
