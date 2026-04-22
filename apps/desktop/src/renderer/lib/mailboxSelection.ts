import type { MailboxGroup, MailboxRow } from "../../shared/types";

export function mailboxRowSelectionId(
  row: Pick<MailboxRow, "id" | "kind" | "attachment_id">,
) {
  if (row.kind === "attachment" && row.attachment_id) {
    return `${row.id}:${row.attachment_id}`;
  }
  return row.id;
}

export function firstMailboxRowSelectionId(groups: MailboxGroup[]) {
  for (const group of groups) {
    for (const row of group.rows) {
      return mailboxRowSelectionId(row);
    }
  }
  return null;
}

export function findMailboxRowBySelectionId(
  groups: MailboxGroup[],
  selectionId: string | null,
) {
  if (!selectionId) {
    return null;
  }
  for (const group of groups) {
    for (const row of group.rows) {
      if (mailboxRowSelectionId(row) === selectionId) {
        return row;
      }
    }
  }
  return null;
}

export function findMailboxSelectionIdByThreadId(
  groups: MailboxGroup[],
  threadId: string | null,
) {
  if (!threadId) {
    return null;
  }
  for (const group of groups) {
    for (const row of group.rows) {
      if (row.thread_id === threadId) {
        return mailboxRowSelectionId(row);
      }
    }
  }
  return null;
}
