# 01 — Schema & Action Taxonomy

Foundational reference. Read after [00-overview.md](./00-overview.md). The full per-action catalog with context shapes lives in [APPENDIX-action-catalog.md](./APPENDIX-action-catalog.md) and [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md).

## Table schema

```sql
-- crates/store/migrations/0NN_user_activity.sql
CREATE TABLE IF NOT EXISTS user_activity (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           INTEGER NOT NULL,                       -- unix epoch ms (i64)
    account_id   TEXT,                                   -- nullable (app-level actions are accountless)
    source       TEXT NOT NULL,                          -- 'tui' | 'cli' | 'web' | 'daemon'
    action       TEXT NOT NULL,                          -- canonical action token (see catalog)
    target_kind  TEXT,                                   -- 'thread' | 'message' | 'draft' | 'search' | ...
    target_id    TEXT,                                   -- foreign-ish id; not enforced (targets may be deleted)
    tier         TEXT NOT NULL DEFAULT 'standard',       -- 'ephemeral' | 'standard' | 'important'
    context_json TEXT,                                   -- JSON, schema per action
    redacted     INTEGER NOT NULL DEFAULT 0,             -- 0 | 1
    CHECK (source IN ('tui', 'cli', 'web', 'daemon')),
    CHECK (tier   IN ('ephemeral', 'standard', 'important')),
    CHECK (redacted IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_user_activity_ts
    ON user_activity (ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_activity_action_ts
    ON user_activity (action, ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_activity_target
    ON user_activity (target_kind, target_id);

CREATE INDEX IF NOT EXISTS idx_user_activity_account_ts
    ON user_activity (account_id, ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_activity_tier_ts
    ON user_activity (tier, ts);    -- used by retention prune (ASC for range scan)
```

## FTS5 mirror

Free-text query over `context_json` (search queries, subjects, recipient handles, draft prefixes, URLs).

```sql
-- crates/store/migrations/0NN_user_activity_fts.sql
CREATE VIRTUAL TABLE IF NOT EXISTS user_activity_fts USING fts5(
    context_json,
    content='user_activity',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS user_activity_ai AFTER INSERT ON user_activity
BEGIN
    INSERT INTO user_activity_fts (rowid, context_json)
    VALUES (new.id, new.context_json);
END;

CREATE TRIGGER IF NOT EXISTS user_activity_ad AFTER DELETE ON user_activity
BEGIN
    INSERT INTO user_activity_fts (user_activity_fts, rowid, context_json)
    VALUES ('delete', old.id, old.context_json);
END;

CREATE TRIGGER IF NOT EXISTS user_activity_au AFTER UPDATE ON user_activity
BEGIN
    INSERT INTO user_activity_fts (user_activity_fts, rowid, context_json)
    VALUES ('delete', old.id, old.context_json);
    INSERT INTO user_activity_fts (rowid, context_json)
    VALUES (new.id, new.context_json);
END;
```

The `UPDATE` trigger covers redaction (we update `context_json` to `NULL` when redacting). On redact, the FTS row is removed and not re-inserted (`NULL` skipped at SQLite level).

## Tier policy

| Tier | What goes here | Default retention | Rationale |
|---|---|---|---|
| `ephemeral` | views, screen opens, palette opens, mailbox switches, focus changes | 30 days | High volume, low retrospective value beyond a month |
| `standard` | searches, draft starts, filter applies, snippet inserts, label/folder navigations | 90 days | Mid-value; matches existing `event_retention_days` default |
| `important` | mail mutations (read/archive/trash/star/send/reply/forward/snooze/label), rule edits, account changes, redactions, retention prunes | 365 days | Permanent record of state-changing actions |

Tier is **assigned by the mapper** based on the action token. The classification table is in [03-capture.md](./03-capture.md#tier-classification).

## Action token grammar

Format: `<noun>.<verb>` — both lower-snake_case.

- **Noun** identifies the target domain: `mail`, `thread`, `draft`, `search`, `view`, `account`, `rule`, `snippet`, `link`, `attachment`, `screener`, `reminder`, `label`, `saved`, `activity`, `app`.
- **Verb** identifies the action: `read`, `archive`, `send`, `open`, `run`, `create`, `update`, `delete`, `pause`, etc.

Naming rules:
- Use the verb the user would say. `mail.archive`, not `mail.set_archived`.
- Prefer present-tense single word. `mail.send`, not `mail.sent`.
- Multi-word verbs use snake_case: `mail.mark_read`, `view.open_palette`.
- Pairs use opposing verbs when they exist: `star`/`unstar`, `archive`/`unarchive`, `trash`/`untrash`, `snooze`/`unsnooze`. Mirror `MessageEventKind`.
- Bulk operations don't get their own action — they emit one row per item *or* one row with `context_json.count` and a target list. See [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md#bulk).

Reserve `activity.*` for meta-events about the log itself:
- `activity.paused`
- `activity.resumed`
- `activity.pruned`
- `activity.redacted`
- `activity.exported`
- `activity.cleared`

## Canonical action list (summary — full catalog in APPENDIX)

### Mail mutations (`important`)
`mail.read`, `mail.unread`, `mail.archive`, `mail.unarchive`, `mail.trash`, `mail.untrash`, `mail.star`, `mail.unstar`, `mail.label`, `mail.unlabel`, `mail.snooze`, `mail.unsnooze`, `mail.move`, `mail.send`, `mail.reply`, `mail.forward`, `mail.unsubscribe`, `mail.mark_spam`, `mail.unmark_spam`.

### Threads & messages (`standard` reads, `important` mutations)
`thread.open`, `thread.close`, `thread.summarize`, `thread.flag_reply_later`, `thread.unflag_reply_later`.

### Drafts (`important`)
`draft.create`, `draft.update`, `draft.discard`, `draft.send`, `draft.save`, `draft.attach`, `draft.detach`.

### Search (`standard`)
`search.run`, `search.save`, `search.delete`, `search.rename`, `saved.open`.

### Views & navigation (`ephemeral`)
`view.open_mailbox`, `view.open_label`, `view.open_screen`, `view.open_palette`, `view.open_settings`, `view.open_activity`, `view.open_analytics`, `view.open_rules`, `view.open_screener`.

### Accounts (`important`)
`account.add`, `account.remove`, `account.rename`, `account.sync`, `account.signin`, `account.signout`.

### Rules (`important`)
`rule.create`, `rule.update`, `rule.delete`, `rule.run`, `rule.test`, `rule.enable`, `rule.disable`.

### Snippets (`standard`)
`snippet.create`, `snippet.update`, `snippet.delete`, `snippet.insert`.

### Links & attachments (`standard`; link opt-in)
`link.click`, `attachment.open`, `attachment.save`.

### Screener (`important`)
`screener.allow`, `screener.block`, `screener.snooze`.

### Reminders (`important`)
`reminder.set`, `reminder.clear`, `reminder.snooze`.

### App lifecycle (`ephemeral`)
`app.start`, `app.stop`.

### Meta — activity log itself (`important`)
`activity.paused`, `activity.resumed`, `activity.pruned`, `activity.redacted`, `activity.exported`, `activity.cleared`.

## Context JSON conventions

- One JSON shape per action. Documented in [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md).
- Fields are flat-as-possible. Nesting only when shape demands it (e.g. `recipients: { to: [], cc: [], bcc: [] }`).
- Recipient handles stored as `{ name, email }` — no public-key material, no thread bodies.
- Subjects stored verbatim, up to 200 chars; longer subjects are truncated with `…`.
- Draft body content stored as **prefix only** — first 80 chars, never the full draft.
- Bulk operations encode the affected set as `context_json.target_ids: [...]` plus `count`, with one activity row representing the batch.

Forbidden in `context_json` — **codify in the recorder**:
- OAuth tokens, refresh tokens, password hashes, API keys.
- Attachment bytes (only filenames + sizes).
- Full mail body text — only subject + first-line snippet (already a column-bounded scalar at the daemon).

## Rust types (target)

```rust
// crates/store/src/user_activity.rs

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActivityRow {
    pub id: i64,
    pub ts: i64,
    pub account_id: Option<String>,
    pub source: String,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tier: String,
    pub context_json: Option<String>,
    pub redacted: i64,
}

#[derive(Debug, Clone)]
pub struct ActivityInsert<'a> {
    pub ts: i64,
    pub account_id: Option<&'a str>,
    pub source: ClientKind,
    pub action: &'a str,
    pub target_kind: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub tier: Tier,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier { Ephemeral, Standard, Important }

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientKind { Tui, Cli, Web, Daemon }
```

`ClientKind` lives in `crates/protocol` (shared with clients), `Tier` lives in `crates/store` (storage-shaped).

## Migration ordering

Migrations are sequential in `crates/store/migrations/`. Check the next free number at implementation time — at the time this doc was written the highest was `032`. Verify with:

```sh
ls crates/store/migrations | tail -5
```

Use the next number for `_user_activity.sql` and `_user_activity_fts.sql`. Apply in that order (FTS depends on the base table).
