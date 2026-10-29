import type { MessageRowView } from "./types";

/**
 * A row's identity in the list. A conversation row is its thread: the
 * message that represents it can change under the cursor (search picks the
 * best-matching message, and a read or archive elsewhere in the thread can
 * change which one that is), and the cursor, selection and DOM id must not
 * move when it does. Attachment search rows share their message's id, one
 * per attachment, so they need the attachment id too.
 */
export function rowKey(row: MessageRowView): string {
  // A legacy thread id can be held by two accounts, so the row's account is
  // part of its identity: selecting one account's row must not select the other's.
  if (row.kind === "thread") return threadRowKey(row.thread_id, row.account_id);
  return row.kind === "attachment" && row.attachment_id ? `${row.id}:${row.attachment_id}` : row.id;
}

/** Identity of a conversation row: the thread within its account. */
export function threadRowKey(threadId: string, accountId?: string | null): string {
  return accountId ? `thread-${accountId}:${threadId}` : `thread-${threadId}`;
}
