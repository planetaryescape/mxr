# Phase 2 — Mailbox + Thread Reader

Goal: the core inbox experience. The user can browse a virtualized mailbox list, select with hover/checkbox/shift-range, perform optimistic mutations with undo, and read threads in the right pane. WebSocket events drive live updates without page reloads.

## Deliverables

1. **Sidebar** populated with system mailboxes (Inbox, Starred, Snoozed, Drafts, Sent, Archive, Spam, Trash, Reply-Later) + label list + saved searches. Live unread counts.
2. **Mailbox list** (virtualized via TanStack Virtual). Density-aware row height. Each row shows star, sender, subject, preview, label chips, paperclip, age.
3. **Selection model**:
   - Hover reveals checkbox.
   - Click row → open thread.
   - Click checkbox / `x` toggles row.
   - Shift-click extends a range.
   - `*a` select all in current view, `*n` invert.
4. **Bulk action bar**: slides up from bottom of list pane when selection is non-empty. Shows N selected + Archive/Trash/Spam/Star/Label/Snooze/Move/MarkRead/MarkUnread/Clear.
5. **Optimistic mutations**: archive, trash, spam, star, mark-read, mark-unread, label add/remove, move, snooze. Rows update instantly; toast offers Undo for 60 s.
6. **Thread reader pane**: collapsed/expanded messages, body fetch, sanitized HTML or plaintext rendering, attachment list, reader-mode toggles.
7. **Right rail panels** (initially: Sender Profile, Thread Summary, Attachments, URL list).
8. **Realtime**: WebSocket events update the cache: `MailUpdated`, `MailRemoved`, `NewMessages`, `LabelCountsUpdated`, `SyncProgress`.

## Bridge endpoints used

- `GET /api/v1/mail/mailbox?account=&label=&page=&page_size=` → paginated envelopes.
- `GET /api/v1/mail/threads/{thread_id}` → thread + messages.
- `GET /api/v1/mail/labels` (or `/api/v1/platform/labels` — verify in generated.ts) → label list with counts.
- `POST /api/v1/mail/mutations/archive` { message_ids } → optimistic.
- `POST /api/v1/mail/mutations/trash` { message_ids }.
- `POST /api/v1/mail/mutations/spam` { message_ids }.
- `POST /api/v1/mail/mutations/star` { message_ids, starred }.
- `POST /api/v1/mail/mutations/read` { message_ids, read }.
- `POST /api/v1/mail/mutations/read-and-archive` { message_ids }.
- `POST /api/v1/mail/mutations/labels` { message_ids, add, remove }.
- `POST /api/v1/mail/mutations/move` { message_ids, target_label }.
- `POST /api/v1/mail/actions/snooze` { message_ids, until }.
- `GET  /api/v1/mail/actions/snooze/presets` → snooze presets for the popover.
- `POST /api/v1/mail/attachments/open` { message_id, part_id }.
- `POST /api/v1/mail/attachments/download` { message_id, part_id }.
- WebSocket: `MailUpdated`, `MailRemoved`, `NewMessages`, `LabelCountsUpdated`.

The shape of these is defined in `crates/protocol/src/types.rs` and surfaced via `src/api/generated.ts`. Always trust generated types over what's documented here.

## Files

```
src/features/mailbox/
  MailboxRoute.tsx                # the page; wraps everything
  MailboxList.tsx                 # virtualized list
  MailboxRow.tsx                  # single envelope row
  MailboxRowSkeleton.tsx
  MailboxEmpty.tsx                # empty state per mailbox kind
  BulkActionBar.tsx
  Selection.tsx                   # hooks/utilities for selection ranges
  useMailboxQuery.ts
  useMailboxRealtime.ts
src/features/thread/
  ThreadRoute.tsx                 # right pane
  ThreadHeader.tsx                # subject, participants, labels
  ThreadMessage.tsx               # single message in thread
  ThreadActionsToolbar.tsx        # reply / archive / etc.
  AttachmentList.tsx
  ThreadReaderToggles.tsx         # html/text, remote content, signature
  useThreadQuery.ts
src/features/sidebar/
  Sidebar.tsx                     # full implementation, replaces stub
  SidebarSection.tsx
  SidebarItem.tsx
  LensesSection.tsx               # saved searches + system mailboxes
  AccountSwitcherInline.tsx
src/components/
  popovers/
    SnoozePopover.tsx
    LabelPickerPopover.tsx
    MovePopover.tsx
    MoreActionsMenu.tsx
src/hooks/
  useOptimisticMailMutation.ts    # the heart of every mutation
  useUndoToast.ts                 # 60s undo affordance
  useDaemonEventsForMailbox.ts
src/lib/
  mailboxKeys.ts                  # query key factory: ["envelopes", { mailbox, account, ... }]
```

## Optimistic mutation pattern

```ts
// useOptimisticMailMutation.ts (sketch)
export function useOptimisticMailMutation<TVars>(action: MailAction) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: TVars) => api.POST(action.endpoint, { body: vars }),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: ["envelopes"] });
      const previous = snapshotEnvelopeCaches(qc);
      mutateEnvelopeCaches(qc, vars, action);
      return { previous };
    },
    onError: (_err, _vars, ctx) => {
      restoreEnvelopeCaches(qc, ctx?.previous);
      toast.error(`${action.label} failed`);
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ["envelopes"] });
      qc.invalidateQueries({ queryKey: ["labels"] });
    },
    onSuccess: (_data, vars) => {
      const undoToken = (_data as any)?.mutation_id;
      if (undoToken && action.undoable) {
        toast.success(`${action.label}d ${vars.message_ids.length}`, {
          action: { label: "Undo", onClick: () => api.POST("/api/v1/mail/undo", { body: { mutation_id: undoToken } }) },
          duration: 60_000,
        });
      }
    },
  });
}
```

The bridge response for archive/trash/spam/markRead/readArchive includes a `mutation_id` that `Undo` accepts (per CLI help string: "The mutation id is printed by archive, trash, spam, mark-read, and read-archive"). Verify in generated types.

## Selection model

- Stored in Zustand `selectionStore` keyed by current mailbox URL: `{ [mailboxKey]: Set<MessageId> }`.
- Bulk bar reads from store. Mutations dispatch over the current set, then clear.
- Range selection: track last-clicked id; shift-click selects every row between last and current in the **rendered** order.
- Keyboard: `x` toggles current focused row; `*a` selects all in current view; `*n` inverts; `Esc` clears.

## Thread reader

- Body fetch uses `GET /api/v1/mail/threads/{id}` which returns the full thread with messages and bodies (per CLAUDE.md, body is a SQLite read on the daemon side — no spinner under 50 ms).
- Rendering modes: text (default) / html (sanitized) / raw (collapsed).
- `sanitizeHtml.ts` wraps DOMPurify with our defaults: drop `<script>`, force `target="_blank" rel="noopener"` on links, strip remote `<img>` src unless remote content is enabled per-thread.
- Inline images: serve via daemon endpoint (the bridge has `/api/v1/mail/attachments/open` and image proxy via `GetHtmlImageAssets`); replace `cid:` references with proxied URLs at render time.
- Toggles: Reader Mode (clean filter), HTML view, Remote Content, Signature visibility. Persist last toggle state per-thread in URL search params.

## Realtime invalidation

```ts
// hooks/useDaemonEventsForMailbox.ts (sketch)
useDaemonEvents((evt) => {
  switch (evt.type) {
    case "NewMessages":
    case "MailUpdated":
    case "MailRemoved":
      qc.invalidateQueries({ queryKey: ["envelopes"] });
      break;
    case "LabelCountsUpdated":
      qc.setQueryData(["labels"], (prev) => mergeLabelCounts(prev, evt));
      break;
    case "SyncProgress":
      useConnectionStore.setState({ syncProgress: evt.progress });
      break;
  }
});
```

## Verification

1. Open `/m/inbox` → list renders, virtualization keeps DOM nodes under 100.
2. Click a row → URL updates to `/m/inbox/$threadId`, right pane shows thread.
3. Hover → checkbox visible. Check 3 rows → bulk bar slides up.
4. Click "Archive" in bulk bar → 3 rows fade out, toast appears with Undo. Click Undo → rows reappear.
5. Network tab shows POST to `/mutations/archive` then (on Undo) POST to `/undo`.
6. With `mxr daemon` running, send a fake new message via `mxr` CLI: row appears in inbox without page reload (`NewMessages` event).
7. Snooze popover: pick "tomorrow 9am" → row disappears, toast "Snoozed for 18h", undo restores.
8. Label popover: type to filter, check 2 labels → POST `/mutations/labels` with `add: ["X","Y"]`.
9. Move popover: pick label → POST `/mutations/move`.
10. Reader pane: switch HTML / Plain / Reader → no remote images load by default; toggle Remote → images load.
11. Attachments → click → fetch + open in browser tab via `/attachments/open`.
12. URL list panel → extracted clickable URLs from message body.
13. Resize tablet width: sidebar collapses to icons, list pane stays usable.
14. WebSocket disconnect (kill daemon) → status pill goes red. Reconnect → pill recovers.

## Decisions made during execution

(Append as work proceeds.)
