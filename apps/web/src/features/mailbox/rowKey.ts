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
  if (row.kind === "thread") return `thread-${row.thread_id}`;
  return row.kind === "attachment" && row.attachment_id ? `${row.id}:${row.attachment_id}` : row.id;
}
