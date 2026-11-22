import { fetchThread } from "@/features/mailbox/api";
import type { MessageLabelView, MessageRowView, ThreadResponse } from "@/features/mailbox/types";
import { getActiveQueryClient } from "@/lib/queryClient";

import { rowMessageIds } from "./pendingMailOps";

export interface MailTarget {
  messageIds: string[];
  unstarIds: string[];
  rows: MessageRowView[];
  conversations: number;
  threadId?: string;
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
  const messageIds =
    data.thread.message_ids && data.thread.message_ids.length > 0
      ? [...new Set(data.thread.message_ids)]
      : messages.map((message) => message.id);
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

export async function ensureThread(threadId: string): Promise<ThreadResponse> {
  const qc = getActiveQueryClient();
  if (!qc) return fetchThread(threadId);
  return qc.fetchQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    staleTime: 30_000,
  });
}
