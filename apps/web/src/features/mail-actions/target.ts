/*
 * What a mail verb acts on. The list builds a target from the selection
 * (or the focused row); the reader builds one from the open thread. Verbs
 * only ever see this shape, so "e" means the same thing in both places.
 */

import { fetchThread } from "@/features/mailbox/api";
import type { MessageLabelView, MessageRowView, ThreadResponse } from "@/features/mailbox/types";
import { getActiveQueryClient } from "@/lib/queryClient";

import { rowMessageIds } from "./pendingMailOps";

export interface MailTarget {
  /** Every message the action covers (whole threads for thread rows). */
  messageIds: string[];
  /**
   * What unstarring the target changes: its messages plus any starred
   * message of its conversations outside them (a starred reply in Sent
   * stars the inbox row, so its unstar has to reach that reply).
   */
  unstarIds: string[];
  /** The rows or messages the user acted on, for previews and copy. */
  rows: MessageRowView[];
  /** Distinct conversations covered: a reader target is one, however many
   * messages it has. */
  conversations: number;
  /** Set when the target is exactly one conversation. */
  threadId?: string;
  /** Newest message of a single-thread target: reply, unsubscribe, export. */
  primary?: MessageRowView;
  accountId?: string;
  anyStarred: boolean;
  anyUnread: boolean;
  labels: MessageLabelView[];
  source: "list" | "reader";
}

export function targetFromRows(rows: MessageRowView[], source: MailTarget["source"]): MailTarget {
  const threadIds = new Set(rows.map((row) => row.thread_id));
  const single = threadIds.size === 1 ? rows[0] : undefined;
  const messageIds = [...new Set(rows.flatMap(rowMessageIds))];
  return {
    messageIds,
    unstarIds: [
      ...new Set([...messageIds, ...rows.flatMap((row) => row.starred_message_ids ?? [])]),
    ],
    rows,
    conversations: threadIds.size,
    threadId: single?.thread_id,
    primary: single,
    accountId: single?.account_id,
    anyStarred: rows.some((row) => row.starred),
    anyUnread: rows.some((row) => row.unread),
    labels: uniqueLabels(rows),
    source,
  };
}

export function targetFromThread(data: ThreadResponse, messages = data.messages): MailTarget {
  const newest = messages.at(-1) ?? messages[0];
  const messageIds = messages.map((message) => message.id);
  return {
    messageIds,
    unstarIds: messageIds,
    rows: messages,
    conversations: 1,
    threadId: data.thread.id,
    primary: newest,
    accountId: data.thread.account_id,
    anyStarred: messages.some((message) => message.starred),
    anyUnread: messages.some((message) => message.unread),
    labels: uniqueLabels(messages),
    source: "reader",
  };
}

export function uniqueLabels(rows: MessageRowView[]): MessageLabelView[] {
  const labels = new Map<string, MessageLabelView>();
  for (const row of rows) {
    for (const label of row.labels ?? []) labels.set(label.id, label);
  }
  return [...labels.values()];
}

/** Load (or reuse) the full thread behind a single-thread target. */
export async function ensureThread(threadId: string): Promise<ThreadResponse> {
  const qc = getActiveQueryClient();
  if (!qc) return fetchThread(threadId);
  return qc.fetchQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    staleTime: 30_000,
  });
}
