# Deleting an email leaves vectors, gists, commitments and files behind

Status: logged, not fixed. Audited at `37f6ff61` (branch `docs/email-modes`,
based on `origin/main` `3da0c119`). BK's real DB has 0 orphan semantic
chunks, so the SQLite cascade works today; the gaps are everything the
cascade cannot reach.

## Two code paths delete messages, and neither tells the semantic engine

Only two statements remove rows from `messages`:

- `delete_messages_by_provider_ids` (`crates/store/src/message.rs:620-639`),
  called only from sync (`crates/sync/src/engine.rs:497-502`) with the
  provider's `deleted_provider_ids`.
- `delete_account` (`crates/store/src/account.rs:273-280`), called only by
  `mxr accounts remove --purge` (`crates/daemon/src/handler/account_config.rs:321-343`).

Every way a message leaves the store goes through one of them, or deletes
the whole data directory:

| Path | How it reaches the store |
|---|---|
| Provider delete or expunge (Gmail) | History `messagesDeleted` (`crates/provider-gmail/src/provider.rs:512-514`) becomes `deleted_provider_ids` (`:563`), then `delete_messages_by_provider_ids` |
| Provider delete or expunge (IMAP) | QRESYNC `VANISHED` (`crates/provider-imap/src/lib.rs:978-985`) or a UID missing from `UID SEARCH ALL` (`:1100-1112`), then the same call |
| `mxr trash` on Gmail | Not a delete. Gmail adds `TRASH`; the daemon mirrors it as a label change (`crates/daemon/src/handler/mutations.rs:1746-1765`). The row and all derived data stay until Gmail expunges it (30 days) and history reports `messagesDeleted` |
| `mxr trash` on IMAP | `apply_trash` MOVEs to the Trash folder (`crates/provider-imap/src/lib.rs:1635-1667`). The source UID vanishes on the next sync, so the original row is deleted; the Trash copy arrives as a new upsert if that folder is synced |
| `mxr delete` / `mxr expunge` / empty trash | Do not exist. `MutationCommand` has no permanent delete (`crates/protocol/src/types.rs:1948-1987`) |
| Account removal with purge | Tantivy docs removed first, then `DELETE FROM accounts` cascades (`account_config.rs:321-343`). Without `--purge` the account is only disabled and everything stays |
| `mxr reset --hard` / `mxr burn` | Deletes `mxr.db`, `search_index`, `models`, `attachments` (only if inside the data dir), `logs`, and any other file in the data dir (`crates/daemon/src/commands/reset.rs:180-270`). Daemon must be stopped first. Complete, except an attachment dir outside the data dir, which is kept on purpose |
| Retention or pruning of messages | None. Retention exists only for `user_activity` (30/90/365 days by tier, `crates/daemon/src/loops.rs:2016-2024`), undo and dedup logs, and `event_log` on manual `mxr logs --purge` |

Two delete events are never seen at all:

- Gmail history expiry. When the cursor is stale, sync resets to an
  initial sync (`crates/sync/src/engine.rs:341-353`), and initial sync
  reports no deletions (`provider.rs:392, 422, 451`). Mail deleted on Gmail
  while mxr was offline longer than the history window (Gmail documents
  "typically at least a week") stays local forever, with all its derived
  data.
- IMAP `UIDVALIDITY` change. The folder is refetched from `1:*`
  (`crates/provider-imap/src/lib.rs:1127-1138`, `:887-893` for Gmail All
  Mail) without deleting rows for the old UIDs, which no longer exist on
  the server.

## The SQLite cascade clears per-message tables, but not thread, contact or file caches

Cascades fire because `foreign_keys=ON` on every pool
(`crates/store/src/pool.rs:36, 48, 70`). The sync path removes Tantivy docs
before the SQL delete (`engine.rs:486-495`, applied at `:557-565`).

Neither delete path enqueues anything downstream. `post_sync_fanout`
(`crates/daemon/src/loops.rs:1102`) runs only when `count > 0` upserts
(`loops.rs:711`) and is passed only upserted ids, so a sync that only
deletes refreshes no contacts, relationship data or semantic index.

| Derived store | Provider delete via sync | Account purge | Reset / burn | Gmail trash |
|---|---|---|---|---|
| `bodies`, `attachments` rows, `message_labels`, `provider_meta`, `message_flags` (reply later), `snoozed`, `message_keywords`, `message_pins`, `message_events`, `reply_pairs`, `calendar_invites` (incl. `raw_ics`), `auto_reminders`, `triage_cache`, `delivery_messages`, `messages_fts` | Cleared (FK cascade / `messages_ad` trigger, `001_initial.sql:205-208`) | Cleared | Cleared | Stays |
| `semantic_chunks` (normalised text, incl. attachment text) and `semantic_embeddings` | Cleared (`004_semantic_search.sql:21, 35`) | Cleared | Cleared | Stays |
| HNSW index in daemon memory (vector plus a 240-char snippet per chunk, `crates/semantic/src/lib.rs:77, 149-168`) | Lingers until the next rebuild. Rebuilds happen only when a profile is marked dirty by an ingest that wrote embeddings or a finished pass (`lib.rs:444, 755, 884`) or on daemon restart (index starts empty, `lib.rs:314`). Not persisted to disk: the crate never calls an `hnsw_rs` dump | Lingers, same rule | Cleared (process stopped) | Stays |
| Tantivy lexical index | Cleared at sync (`engine.rs:486-495`). Body text is indexed, not stored (`crates/search/src/schema.rs:51-62`); deleted terms stay in segment files until a merge | Cleared | Cleared | Stays |
| Attachment files on disk (`attachment_dir/<message_id>/`, `crates/daemon/src/handler/mod.rs:3418`) | Lingers forever. Nothing in daemon, store or sync removes them | Lingers forever | Cleared if inside the data dir | Stays |
| Thread gists (`context_briefings` kind `thread`, read by `cached_gist`, `crates/daemon/src/handler/thread_gist.rs:196-222`) | Lingers (account FK only, `032_context_briefings.sql:8`). A partly deleted thread misses on `content_hash` and regenerates when viewed; a wholly deleted thread keeps its gist text forever | Cleared | Cleared | Stays |
| Recipient briefings (`context_briefings` kind `recipient`) | Lingers until regenerated | Cleared | Cleared | Stays |
| `thread_summaries` (`mxr summarize` text, `021_thread_summaries.sql`) | Lingers forever for a deleted thread | Cleared | Cleared | Stays |
| `decision_log` (decision and rationale text, `evidence_msg_ids` JSON, `034_decision_log_stable_id.sql`) | Lingers and is still listed by `mxr decisions` | Cleared | Cleared | Stays |
| `contact_commitments` (`what` text, `evidence_msg_id` without FK, `024_contact_commitments.sql`) | Lingers and is still listed | Cleared | Cleared | Stays |
| `deliveries` (merchant, items, tracking, `042_deliveries.sql`) | Provenance rows cascade; the delivery row stays with no source mail | Cleared | Cleared | Stays |
| `desk_dismissals` (watermark only, no text, `051_desk_dismissals.sql`) | Lingers, harmless | Cleared | Cleared | Stays |
| Deferrals (`message_flags.reply_later_*`, `auto_reminders`, `snoozed`) | Cleared (cascade) | Cleared | Cleared | Stays |
| `contacts` (display name, counts, dates) | Lingers. Refresh runs only after upserts, and is `INSERT OR REPLACE` that never deletes a contact whose mail is all gone (`crates/store/src/contacts.rs:46, 193`) | Cleared | Cleared | Stays |
| `contact_relationship_summary` (model-written text about the person) | Lingers; regenerated only when the contact is re-enqueued by new mail (`crates/relationship/src/summary.rs:44-47`) | Cleared | Cleared | Stays |
| `contact_style` | Lingers with stale metrics. No free text: openers and sign-offs are whitelisted words only (`crates/relationship/src/stylometry.rs:164-197`) | Cleared | Cleared | Stays |
| `user_voice_profile` | Lingers until the next upsert-triggered rebuild; holds `exemplar_message_ids` (ids, not text, `crates/relationship/src/user_voice.rs:26`) | Cleared | Cleared | Stays |
| `history_text_fingerprints` | 64-bit hashes only, pruned after 7 days (`055_history_text_fingerprints.sql`). Acceptable | Cleared | Cleared | Stays |
| `event_log` | Lingers with subjects in `summary` ("Applied rules to <subject>", `loops.rs:1358-1363`; "Unsnoozed '<subject>'", `mutations.rs:2157`). Pruned only by manual `mxr logs --purge` | Cleared | Cleared | Stays |
| `user_activity` (`context_json` and FTS mirror) | Lingers until tier retention. Message targets are ids; draft actions carry the draft subject up to 200 chars (`crates/daemon/src/activity/mapper.rs:24, 377`), which for a reply repeats the deleted subject | Rows with `account_id` stay (no FK) | Cleared | Stays |
| `rule_execution_log`, `sent_draft_receipts`, `mutation_undo_log`, `mutation_dedup_log` | Ids only, no FK. Linger, no text | Mostly linger | Cleared | Stays |
| SQLite free pages and WAL | Deleted text stays in free pages until reused or vacuumed; no `secure_delete` or `auto_vacuum` is set | Same | Cleared | Stays |

## Deleted mail does not surface in search results today, but its vectors still rank

Both semantic consumers hydrate and drop hits whose message is gone:
`filter_dense_hits` skips a hit with no envelope
(`crates/daemon/src/handler/diagnostics/search_execute.rs:718`), and `mxr
ask` skips candidates where `get_envelope` returns nothing
(`crates/daemon/src/handler/archive_ask.rs:86-89`). `SemanticHit.snippet`
is never sent to a client; it is built in `best_hits_for_neighbours`
(`crates/semantic/src/lib.rs:2002`) and dropped by both callers. So a
deleted message's text does not appear in results or snippets.

What remains:

- The deleted chunk's 240-char snippet and vector sit in daemon memory
  until the next rebuild, which on a quiet account can be days.
- Deleted vectors still take ANN candidate slots. Search asks for a fixed
  candidate window and collapses to one hit per message; tombstoned
  neighbours mean fewer live hits, so a heavy delete (a purge of a
  newsletter) can push live matches out of the window.
- A rebuild already running when the delete lands was reading rows from
  before it, so it can reinstate the deleted chunks when it swaps in.

## Trash should keep vectors; permanent delete must clear them

Gmail's Trash is reversible and Gmail permanently deletes trashed mail
after 30 days. That expunge arrives as a history `messagesDeleted`, which
mxr already turns into a local delete, so tying vector removal to the row
delete gets the right moment without a separate trash rule. Clearing
vectors on trash would make untrash need a re-embed and would make a trashed
message unsearchable while it is still in the user's mailbox.

One open question for BK: should trashed and spam mail be excluded from
semantic results and from model work (gists, briefings, mode
classification) by default? I found no default trash filter in
`search_execute.rs`. That is a relevance choice, separate from the
deletion invariant.

## Fixes, smallest first

1. **Forget deleted messages in the semantic engine.** Add
   `SemanticCommand::ForgetMessages { message_ids }`
   (`crates/semantic/src/service.rs:40-80`). It removes those messages'
   entries from `chunks_by_id` in every loaded `SemanticIndex` at once
   (drops snippet and message mapping; `best_hits_for_neighbours` already
   skips unknown point ids), keeps the ids in a forget set applied to any
   build that finishes afterwards, and marks the profile dirty so the next
   rebuild drops the vectors. `hnsw_rs` has no point removal, so the
   rebuild is the only way to drop the vector itself.
2. **Report deleted ids out of sync.** `SyncOutcome` carries the
   `removed_message_ids` the engine already collects (`engine.rs:486-495`).
   The sync loop runs a delete fan-out even when `count == 0`: semantic
   forget, attachment cleanup, derived-data purge, contacts refresh and
   relationship re-enqueue for the counterparties. The account purge path
   calls the same fan-out with `list_message_ids_by_account`.
3. **Delete attachment files.** Remove `attachment_dir/<message_id>/` for
   each deleted id after the SQL commit. Log and continue on I/O errors.
4. **Purge derived rows in the same transaction as the delete.** One store
   function, `delete_messages_and_derived(account_id, provider_ids)`,
   replaces `delete_messages_by_provider_ids`. Before the `DELETE` it
   collects message ids, thread ids and counterparties, then in one
   transaction:
   - deletes `context_briefings` (kind `thread`) and `thread_summaries` for
     every touched thread, and `desk_dismissals` for threads left with no
     messages;
   - deletes `decision_log` rows whose `evidence_msg_ids` contains a
     deleted id (`json_each`), and `contact_commitments` with a deleted
     `evidence_msg_id`;
   - deletes `deliveries` left with no `delivery_messages`;
   - deletes `contact_relationship_summary` and recipient briefings for the
     counterparties (regenerated on demand), and `event_log` rows with a
     deleted `message_id`;
   - deletes the message rows (cascade handles the rest).
   Make the rule-applied event carry `message_id` via `insert_event_refs`
   so it is covered, instead of only a subject.
5. **Contacts drop people with no mail.** `refresh_contacts` deletes
   `contacts` rows absent from the aggregate, or the delete fan-out deletes
   them for affected emails.
6. **Reconcile after a full resync.** After an initial sync triggered by
   `SyncCursorExpired` or a `UIDVALIDITY` change completes, delete local
   rows for that account (or folder) whose provider id the resync did not
   see, through the same path as 4. This is the largest change: it needs
   the resync to record seen ids and must not run after a partial or
   failed pass.
7. **Optional, privacy hardening.** `PRAGMA secure_delete=ON` (or
   periodic `incremental_vacuum`) so deleted text does not survive in free
   pages. BK's call on the write cost.

## Blueprint 22's new tables each need a deletion rule before they ship

Proposed invariant text for `docs/blueprint/22-email-modes.md`, in "Data
the modes need":

> **Deleting an email deletes everything derived from it, including
> vectors.** When a message leaves the store (provider expunge, account
> purge, reconciliation), every row, cache, file and index entry built
> from its content goes with it in the same delete path: chunks, chunk
> mode tags, embeddings no other chunk references, ANN entries, model
> caches, extracted facts and fields, fetched articles no other item
> references, and gists of its thread. Rows the user made survive only
> when they hold no text copied from the deleted message. Trash is not
> delete: derived data stays until the provider expunges. Every new table
> keyed by message, thread or content says which rule it follows, and
> ships with a test that deletes the source message and finds nothing
> left.

Per table:

| Proposed store | Keyed by | What deletion needs |
|---|---|---|
| `todos` | `source_message_id`, `thread_id`, `done_message_id` (no FKs in the sketch) | Detected to-dos (`origin` rule, schema, model) are deleted with their source message: `source_message_id` FK with `ON DELETE CASCADE`. `done_message_id` gets `ON DELETE SET NULL`. Manual to-dos have no source. Handoff to-dos are user-made but copy `due_words`, `title`, `action_url` and `reason` from the email: either delete them too, or null the copied fields. BK decides |
| `records` | composite, built from many emails | Not cascadable from one message. After a delete, a record with no `record_messages` left is deleted unless the user created or edited it; a surviving record recomputes its fields from the remaining sources |
| `record_fields` | record | FK to `records` with cascade. Each field stores its source message id with `ON DELETE CASCADE` (or SET NULL plus recompute), so a field quoted from a deleted email does not outlive it |
| `record_messages` | record, message | Both FKs `ON DELETE CASCADE`, as `delivery_messages` does. Drives the record GC above |
| `update_facts` | `message_id` | `REFERENCES messages(id) ON DELETE CASCADE`, as `triage_cache` (`044_triage_cache.sql:14`) |
| `update_sources` | account, `source_key` | User tuning, no message text. Account cascade only. Correct as proposed |
| `update_trackers` (if built) | entity | Same GC as `deliveries`: delete when no source messages remain |
| `reading_items` | `message_id`, `idx` | `ON DELETE CASCADE` on message |
| `reading_articles` | `url_unwrapped`, shared | Reference-counted. Delete an article when no `reading_items`, `reading_state` (later) or highlight points at its URL. Run the GC in the delete transaction, not on a timer |
| `reading_state`, `reading_highlights` | `item_key` | User-made, but a highlight's `quote` is the email's or article's text. Deleting the source deletes its highlights; BK decides whether a highlight on an article survives once the article is only referenced by the highlight |
| `mode_aspects` | message, prompt version, content hash | Message FK `ON DELETE CASCADE` |
| `mode_corrections` | scope `message` or `sender` | `message`-scoped rows deleted with the message; `sender` rows are user policy and stay |
| `mode_done` | thread watermark | Like `desk_dismissals`: no text, delete when the thread has no messages left |
| `people` | identity | Recompute on delete; drop a person with no remaining mail and no user merge |
| Per-mode chunk tags (`chunk_modes`) | `chunk_id`, mode | FK to the chunk with `ON DELETE CASCADE` |
| Embeddings keyed by text hash, shared across messages | `(text_hash, profile_id)` | A cascade cannot reach them: chunks point at the embedding, not the other way. After deleting chunks, delete each embedding whose `text_hash` no remaining chunk references (`NOT EXISTS` over `semantic_chunks`), in the same transaction. An `AFTER DELETE ON semantic_chunks` trigger works too, if a test proves it fires for cascade deletes. The ANN then maps one point to many messages: forgetting a message removes it from the point's message set, and the point goes only when the set is empty |
| Gist chunks (gist text indexed per message) | thread | The gist chunk must hang off a message (or a thread row that is deleted with the thread's last message), and must be dropped when any message it summarises is deleted, since the gist can quote it |

## Tests to add

- Store: deleting a message by provider id removes its chunks and
  embeddings (exists implicitly; make it explicit), thread gist and
  summary, decision rows citing it, commitments citing it, and a delivery
  left without sources; a sibling message in the same thread keeps its own
  rows. `crates/store/src/tests.rs`.
- Store: account purge leaves no row in any table with that `account_id`
  or any of its message ids. Generate the table list from
  `sqlite_master` so a new table fails the test until it declares its rule.
- Semantic: ingest two messages, forget one, search returns only the other
  and `chunks_by_id` has no entry for the forgotten one; a build started
  before the forget does not reinstate it. `crates/semantic/src/lib.rs`
  tests.
- Sync: a delete-only batch triggers the delete fan-out (`count == 0`
  path), and the attachment directory for the deleted message is removed.
  `crates/sync` and `crates/daemon/src/loops.rs` tests with the fake
  provider.
- Reconciliation: after `SyncCursorExpired` and a full resync that omits a
  message, the local row and its derived data are gone; a resync that
  fails midway deletes nothing.
- Blueprint 22: one test per new table that deletes the source message
  and asserts nothing derived remains, and a shared-embedding test where
  two messages share a text hash, one is deleted, the embedding stays,
  the second is deleted, the embedding goes.
