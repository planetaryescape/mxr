import type { MessageRowView } from "./types";

/**
 * A row's identity in the list. Attachment search rows share their
 * message's id, one per attachment, so they need the attachment id too.
 */
export function rowKey(row: MessageRowView): string {
  return row.kind === "attachment" && row.attachment_id ? `${row.id}:${row.attachment_id}` : row.id;
}
