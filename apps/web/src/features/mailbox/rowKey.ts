import type { MessageRowView } from "./types";

export function rowKey(row: MessageRowView): string {
  if (row.kind === "thread") return `thread-${row.thread_id}`;
  return row.kind === "attachment" && row.attachment_id ? `${row.id}:${row.attachment_id}` : row.id;
}
