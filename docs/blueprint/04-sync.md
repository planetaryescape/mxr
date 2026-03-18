# mxr — Sync Engine

## Overview

The sync engine orchestrates data flow between providers and local state (SQLite + Tantivy). It runs inside the daemon as a continuous background process.

## Sync lifecycle

### Initial sync (first time an account is added)

1. Authenticate with provider
2. Fetch all labels/folders → upsert into `labels` table
3. Fetch messages in batches (newest first, paginated)
   - Gmail: `messages.list` with `maxResults=100`, paging through all messages
   - Store each batch of envelopes in SQLite
   - Index each batch in Tantivy
   - Parse `List-Unsubscribe` header and store on envelope
4. Store the initial sync cursor (Gmail: latest `historyId`)
5. Log sync completion in `sync_log`

Initial sync for a large mailbox (10k+ messages) may take several minutes. The daemon should:
- Sync in batches, committing each batch to SQLite (no single giant transaction)
- Make messages available in the TUI as they arrive (progressive loading)
- Show sync progress via the IPC protocol (TUI displays a progress indicator)

### Delta sync (subsequent syncs)

1. Read stored sync cursor for account
2. Call provider's `sync_messages(cursor)` → get `SyncBatch`
   - Gmail: `history.list` with `startHistoryId` → returns only changes since last sync
   - This is what makes Gmail sync fast: typically a handful of API calls even for active inboxes
3. Apply `SyncBatch` to local store:
   - Upsert new/modified envelopes
   - Delete messages marked as deleted
   - Apply label changes
   - Parse `List-Unsubscribe` on new messages
4. Update Tantivy index (add new docs, remove deleted)
5. Update sync cursor in `accounts` table
6. Update label counts (unread_count, total_count)
7. Notify connected TUI clients of changes via IPC
8. Log sync in `sync_log`

### Sync loop timing

```rust
async fn sync_loop(store: Store, providers: Vec<Box<dyn MailSyncProvider>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60)); // configurable
    loop {
        interval.tick().await;
        for provider in &providers {
            if let Err(e) = sync_account(&store, provider).await {
                tracing::error!(account = %provider.account_id(), "Sync failed: {}", e);
                // Store error in sync_log, notify TUI
            }
        }
    }
}
```

Default: sync every 60 seconds. Configurable per-account. The TUI can also trigger immediate sync via `mxr sync` or a keybinding.

### Error handling

Sync errors should NOT crash the daemon. They should:
- Be logged in `sync_log` with the error message
- Be reported to connected TUI clients (show a status indicator)
- Be retried on the next sync cycle
- Escalate to user notification if errors persist (e.g., auth expired, needs re-auth)

### Conflict resolution

For the v1, use a simple strategy: **last-write-wins with provider as authority**.

If a message was modified both locally and remotely between syncs:
- Remote state wins for server-managed metadata (labels, read status)
- Local-only state is preserved (snooze, draft progress, saved searches)

This is simpler than full CRDT-style conflict resolution and correct for the common case. The ProviderMeta table stores the last-known remote state for conflict detection.

### Cursor invalidation

If the sync cursor becomes invalid (Gmail: historyId too old, which happens if you don't sync for a long time), the provider returns an error. The sync engine should:
1. Log the invalidation
2. Fall back to a full re-sync
3. Inform the user that a full re-sync is happening (it will be slower)
4. This should be rare in practice if the sync interval is reasonable

## Body fetch (lazy hydration)

Message bodies are NOT synced eagerly. Only envelopes (headers/metadata) are synced. Bodies are fetched on demand when the user opens a message.

```
User opens message in TUI
  → TUI sends GetBody { message_id } to daemon
  → Daemon checks bodies table
  → If cached: return immediately
  → If not cached:
    → Call provider.fetch_body(provider_message_id)
    → Parse MIME (via mail-parser)
    → Extract text/plain, text/html, attachment metadata
    → Store in bodies table + attachments table
    → Index body text in Tantivy (update existing doc)
    → Return to TUI
```

This approach means:
- Initial sync is fast (headers only)
- Storage grows incrementally (only messages you actually read)
- Offline access works for previously read messages
- Tantivy index becomes richer over time as bodies are fetched

## Snooze wake loop

Runs alongside the sync loop in the daemon.

```rust
async fn snooze_waker(store: Store, sync_engine: SyncEngine) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let now = Utc::now();
        let due = store.get_due_snoozes(now).await;
        for snoozed in due {
            // 1. Re-apply INBOX label on provider
            //    (Gmail: POST /messages/{id}/modify addLabelIds: ["INBOX"])
            sync_engine.unsnooze(&snoozed).await;

            // 2. Restore local labels
            store.restore_labels(&snoozed).await;

            // 3. Remove from snoozed table
            store.remove_snooze(&snoozed.message_id).await;

            // 4. Notify connected TUI clients
            notify_clients(Event::MessageUnsnoozed {
                message_id: snoozed.message_id
            });
        }
    }
}
```

### Snooze keybinding flow

```
User presses Z on a message
  → Snooze menu appears:
    t = tomorrow 9am
    n = next Monday 9am
    w = this weekend (Saturday 10am)
    e = tonight (6pm)
    c = custom datetime prompt
  → User selects option
  → Daemon:
    1. Records current labels for the message
    2. Removes INBOX label on Gmail (archive)
    3. Inserts row into snoozed table
    4. Message disappears from inbox view
```

When snooze wakes, the message reappears in both mxr and Gmail's web UI. This is critical for inbox-zero workflows — the state must be consistent across clients.

## Attachment handling

Attachments are metadata-only until downloaded:

```
User views message with attachments
  → TUI shows: [1] invoice.pdf (2.3 MB)  [2] receipt.png (145 KB)
  → User presses 'a' then '1'
  → Daemon calls provider.fetch_attachment(message_id, attachment_id)
  → Raw bytes saved to configurable download directory
  → local_path updated in attachments table
  → User can then open with 'o' (xdg-open) or see the file path
```

Download directory default: `~/mxr/attachments/` (configurable).

## Sync diagnostics

The `sync_log` table provides a history of sync operations for debugging:

```sql
-- Recent sync history for an account
SELECT started_at, finished_at, status, messages_synced, error_message
FROM sync_log
WHERE account_id = ?
ORDER BY started_at DESC
LIMIT 20;
```

`mxr doctor` reads this and reports:
- Last successful sync per account
- Any recurring errors
- Sync duration trends
- Cursor validity
