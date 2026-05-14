# Appendix — Action Catalog

Canonical, exhaustive list of action tokens. Source of truth for the mapper ([03-capture.md](./03-capture.md)) and per-action formatters (TUI [06-tui.md](./06-tui.md), web [07-web.md](./07-web.md)).

Format: `action.token` → tier · emitters · target_kind · context shape (see [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md) for full JSON).

Emitter codes: `T`=TUI, `C`=CLI, `W`=Web, `D`=Daemon-synthesized.

## Mail mutations (`important`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `mail.read` | important | T C W | thread | Single or bulk; bulk encoded in context |
| `mail.unread` | important | T C W | thread | |
| `mail.archive` | important | T C W | thread | |
| `mail.unarchive` | important | T C W | thread | |
| `mail.trash` | important | T C W | thread | |
| `mail.untrash` | important | T C W | thread | |
| `mail.star` | important | T C W | thread | |
| `mail.unstar` | important | T C W | thread | |
| `mail.label` | important | T C W | thread | Context carries label name |
| `mail.unlabel` | important | T C W | thread | |
| `mail.move` | important | T C W | thread | Context carries `to` label |
| `mail.snooze` | important | T C W | thread | Context carries `until` (unix ms) |
| `mail.unsnooze` | important | T C W | thread | |
| `mail.send` | important | T C W | draft | Context carries recipients summary + subject |
| `mail.reply` | important | T C W | thread | Context carries `draft_id` |
| `mail.forward` | important | T C W | thread | Context carries `draft_id` |
| `mail.unsubscribe` | important | T C W | thread | Context carries inferred unsubscribe URL (no traversal) |
| `mail.mark_spam` | important | T C W | thread | |
| `mail.unmark_spam` | important | T C W | thread | |

## Thread navigation & summary

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `thread.open` | standard | T C W | thread | One row when user opens a thread reader |
| `thread.close` | ephemeral | T W | thread | TUI/web close; CLI doesn't emit |
| `thread.summarize` | standard | T C W | thread | LLM summary run |
| `thread.flag_reply_later` | important | T C W | thread | |
| `thread.unflag_reply_later` | important | T C W | thread | |

## Drafts (`important`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `draft.create` | important | T C W | draft | Context carries recipients prefix, subject prefix |
| `draft.update` | standard | T W | draft | Coalesced by compaction (Phase 9) |
| `draft.discard` | important | T C W | draft | |
| `draft.send` | important | T C W | draft | Distinct from `mail.send`; this is the local discard-on-send marker. Pick one — see decision note below. |
| `draft.save` | standard | T C W | draft | Manual save |
| `draft.attach` | standard | T C W | draft | Context: file name + size |
| `draft.detach` | standard | T C W | draft | |

**Decision note**: keep both `draft.send` and `mail.send`. `draft.send` fires when the draft is dispatched locally; `mail.send` fires on provider confirmation. Two distinct events because they can be seconds apart and either can fail independently. Document this in the user guide.

## Search

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `search.run` | standard | T C W | search | Context: query, result_count, mode (lexical|semantic|hybrid) |
| `search.save` | important | T C W | search | Context: slug, name, query |
| `search.delete` | important | T C W | search | |
| `search.rename` | standard | T C W | search | |
| `saved.open` | standard | T C W | search | Distinct from `search.run`: opens a saved query |

## Views & navigation (`ephemeral`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `view.open_mailbox` | ephemeral | T W | label | inbox/starred/sent/etc. |
| `view.open_label` | ephemeral | T W | label | Custom label |
| `view.open_screen` | ephemeral | T W | — | Context: screen name |
| `view.open_palette` | ephemeral | T W | — | |
| `view.open_settings` | ephemeral | T W | — | Context: section |
| `view.open_activity` | ephemeral | T W | — | Self-reference; safe |
| `view.open_analytics` | ephemeral | T W | — | |
| `view.open_rules` | ephemeral | T W | — | |
| `view.open_screener` | ephemeral | T W | — | |

CLI does **not** emit `view.*` — it has no concept of "view"; every CLI invocation is a discrete action that maps to its own action token.

## Accounts (`important`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `account.add` | important | C W | account | Context: account label, provider (no credentials) |
| `account.remove` | important | C W | account | |
| `account.rename` | important | C W | account | |
| `account.sync` | important | C W | account | User-initiated sync only — auto-syncs are NOT emitted |
| `account.signin` | important | C W | account | OAuth completion; never carries token material |
| `account.signout` | important | C W | account | |

## Rules (`important`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `rule.create` | important | T C W | rule | Context: rule name |
| `rule.update` | important | T C W | rule | |
| `rule.delete` | important | T C W | rule | |
| `rule.run` | important | T C W | rule | Manual run; auto-runs do NOT emit |
| `rule.test` | standard | T C W | rule | Dry-run test |
| `rule.enable` | important | T C W | rule | |
| `rule.disable` | important | T C W | rule | |

## Snippets (`standard`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `snippet.create` | standard | T C W | snippet | |
| `snippet.update` | standard | T C W | snippet | |
| `snippet.delete` | standard | T C W | snippet | |
| `snippet.insert` | standard | T W | snippet | Inserted into a draft; context: draft_id, snippet slug |

## Links & attachments

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `link.click` | standard | T W | — | **Opt-in only** (`activity.track_link_clicks=true`). Context: url, from_message_id |
| `attachment.open` | standard | T W | message | Context: filename, size, mime |
| `attachment.save` | standard | T C W | message | Context: filename, size, mime, saved_to (path) |

## Screener (`important`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `screener.allow` | important | T C W | sender | Context: sender email/domain |
| `screener.block` | important | T C W | sender | |
| `screener.snooze` | important | T C W | sender | |

## Reminders (`important`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `reminder.set` | important | T C W | thread | Context: when (unix ms), kind |
| `reminder.clear` | important | T C W | thread | |
| `reminder.snooze` | important | T C W | thread | |

## App lifecycle (`ephemeral`)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `app.start` | ephemeral | T C W | — | Daemon emits one per client connection. Context: client version |
| `app.stop` | ephemeral | T C W | — | On clean disconnect |

## Meta (about the activity log itself)

| Action | Tier | Emitters | target_kind | Notes |
|---|---|---|---|---|
| `activity.paused` | important | D | — | Context: until (unix ms or null) |
| `activity.resumed` | important | D | — | Context: scheduled (bool — true if auto-resumed) |
| `activity.pruned` | important | D | — | Context: tier, before_ts, deleted |
| `activity.redacted` | important | D | — | Context: count, by ("ids" or "filter") |
| `activity.exported` | important | D | — | Context: path, format, filter_summary, count |
| `activity.cleared` | important | D | — | Context: range, affected |

## Tier summary

Default retention:
- `ephemeral` — 30 days
- `standard` — 90 days
- `important` — 365 days

All configurable per tier in `mxr.toml` under `[activity.retention]`.

## Adding a new action

1. Add a row in this table.
2. Add a JSON schema entry in [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md).
3. Add a mapper arm in `crates/daemon/src/activity/mapper.rs`. Exhaustive `match` will fail compile until done.
4. Confirm tier is correct in `crates/daemon/src/activity/tier.rs`.
5. Add a formatter in `crates/tui/src/screens/activity/list.rs` and `apps/web/src/lib/activityFormatters.ts` (optional — `__default` handles unknowns).
6. Add a replay template in `crates/daemon/src/cli/replay_templates.rs` if the action belongs in narrative output.
7. Bump tests.
