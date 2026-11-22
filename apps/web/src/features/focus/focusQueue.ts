import type { DeskRow } from "@/features/desk/api";
import { rowAge } from "@/features/desk/deskCopy";
import type { ReplyQueueMessage } from "@/features/reply-queue/api";

export interface FocusItem {
  threadId: string;
  messageId: string;
  sender: string;
  subject: string;
  reason: string;
  source: "owed" | "reply-later";
}

export function buildFocusQueue(
  owed: readonly DeskRow[],
  replyLater: readonly ReplyQueueMessage[],
  accountScope: string | null,
): FocusItem[] {
  const items: FocusItem[] = [];
  const seen = new Set<string>();
  for (const row of owed) {
    if (seen.has(row.thread_id)) continue;
    seen.add(row.thread_id);
    items.push({
      threadId: row.thread_id,
      messageId: row.message_id,
      sender: row.counterparty_name?.trim() || row.counterparty_email,
      subject: row.subject,
      reason: owedReason(row),
      source: "owed",
    });
  }
  for (const message of replyLater) {
    if (accountScope && message.account_id && message.account_id !== accountScope) continue;
    if (seen.has(message.thread_id)) continue;
    seen.add(message.thread_id);
    items.push({
      threadId: message.thread_id,
      messageId: message.id,
      sender: message.from?.name?.trim() || message.from?.email || "Unknown sender",
      subject: message.subject,
      reason: "In your reply-later queue",
      source: "reply-later",
    });
  }
  return items;
}

export interface FocusSession {
  items: Record<string, FocusItem>;
  queue: string[];
  handled: string[];
  deferred: string[];
  loaded: boolean;
}

export type FocusEvent =
  | { kind: "sync"; items: FocusItem[] }
  | { kind: "skip" }
  | { kind: "handled"; threadId: string }
  | { kind: "restore"; threadId: string }
  | { kind: "front"; threadId: string }
  | { kind: "revisit" };

export const emptyFocusSession: FocusSession = {
  items: {},
  queue: [],
  handled: [],
  deferred: [],
  loaded: false,
};

export function focusReducer(session: FocusSession, event: FocusEvent): FocusSession {
  switch (event.kind) {
    case "sync": {
      const items = { ...session.items };
      const inSource = new Set<string>();
      for (const item of event.items) {
        items[item.threadId] = item;
        inSource.add(item.threadId);
      }
      const current = session.queue[0];
      const kept = session.queue.filter((id) => id === current || inSource.has(id));
      const known = new Set([...kept, ...session.handled, ...session.deferred]);
      const arrivals = event.items.map((item) => item.threadId).filter((id) => !known.has(id));
      const queue = [...kept, ...arrivals];
      if (
        session.loaded &&
        sameIds(queue, session.queue) &&
        event.items.every((item) => sameItem(session.items[item.threadId], item))
      ) {
        return session;
      }
      return { ...session, items, queue, loaded: true };
    }
    case "skip": {
      const [current, ...rest] = session.queue;
      if (current === undefined) return session;
      if (rest.length === 0)
        return { ...session, queue: [], deferred: [...session.deferred, current] };
      return { ...session, queue: [...rest, current] };
    }
    case "revisit": {
      if (session.deferred.length === 0) return session;
      return { ...session, queue: [...session.queue, ...session.deferred], deferred: [] };
    }
    case "handled": {
      if (!session.queue.includes(event.threadId)) return session;
      return {
        ...session,
        queue: session.queue.filter((id) => id !== event.threadId),
        handled: [...session.handled, event.threadId],
      };
    }
    case "front": {
      if (!session.queue.includes(event.threadId) || session.queue[0] === event.threadId)
        return session;
      return {
        ...session,
        queue: [event.threadId, ...session.queue.filter((id) => id !== event.threadId)],
      };
    }
    case "restore": {
      if (!session.handled.includes(event.threadId)) return session;
      return {
        ...session,
        queue: [event.threadId, ...session.queue],
        handled: session.handled.filter((id) => id !== event.threadId),
      };
    }
  }
}

export interface FocusProgress {
  position: number;
  total: number;
  done: number;
}

export function focusProgress(session: FocusSession): FocusProgress {
  const total = session.queue.length + session.handled.length + session.deferred.length;
  const done = session.handled.length;
  return { position: Math.min(done + 1, total), total, done };
}

function sameIds(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((id, index) => id === b[index]);
}

function sameItem(a: FocusItem | undefined, b: FocusItem): boolean {
  return (
    a !== undefined &&
    a.messageId === b.messageId &&
    a.sender === b.sender &&
    a.subject === b.subject &&
    a.reason === b.reason &&
    a.source === b.source
  );
}

function owedReason(row: DeskRow): string {
  const age = rowAge(row);
  const parts = [row.reason.charAt(0).toUpperCase() + row.reason.slice(1), age.label, age.usual];
  return parts.filter(Boolean).join(" · ");
}
