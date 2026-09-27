/*
 * The focus & reply queue: everyone you owe a reply, one conversation per
 * entry. Owed replies come first in the daemon's order (most overdue by
 * your usual cadence with that person), then the reply-later queue in its
 * order, skipping conversations already listed.
 *
 * The session is a pure reducer so its rules are tested without React:
 * the order is fixed when you start, so a refetch never reshuffles what's
 * next; new arrivals join the end; handled conversations never come back
 * unless a send is undone; the one on screen stays put even if a refetch
 * drops it (you may be halfway through the reply).
 */

import type { OwedReplyRow } from "@/features/owed/api";
import { waitingLine } from "@/features/owed/waitingLine";
import type { ReplyQueueMessage } from "@/features/reply-queue/api";

export interface FocusItem {
  threadId: string;
  /** The message a reply answers: the newest from them. */
  messageId: string;
  sender: string;
  subject: string;
  /** Why it is here, in the daemon's words. */
  reason: string;
  source: "owed" | "reply-later";
}

export function buildFocusQueue(
  owed: readonly OwedReplyRow[],
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
      messageId: row.latest_inbound_msg_id,
      sender: row.from_name?.trim() || row.from_email,
      subject: row.subject,
      reason: waitingLine(row),
      source: "owed",
    });
  }
  for (const message of replyLater) {
    // The reply-later list isn't account scoped on the wire; scope it here.
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
  /** Every conversation seen this session, by thread id. */
  items: Record<string, FocusItem>;
  /** Still to do, in order; the first is on screen. */
  queue: string[];
  /** Sent (or in its undo window) or snoozed, in the order handled. */
  handled: string[];
  /** The source has answered at least once. */
  loaded: boolean;
}

export type FocusEvent =
  | { kind: "sync"; items: FocusItem[] }
  | { kind: "skip" }
  | { kind: "handled"; threadId: string }
  | { kind: "restore"; threadId: string };

export const emptyFocusSession: FocusSession = { items: {}, queue: [], handled: [], loaded: false };

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
      const known = new Set([...kept, ...session.handled]);
      const arrivals = event.items.map((item) => item.threadId).filter((id) => !known.has(id));
      const queue = [...kept, ...arrivals];
      // A refetch that changes nothing keeps the same session, so the page
      // and the conversation on screen don't re-render.
      if (
        session.loaded &&
        sameIds(queue, session.queue) &&
        event.items.every((item) => sameItem(session.items[item.threadId], item))
      ) {
        return session;
      }
      return { items, queue, handled: session.handled, loaded: true };
    }
    case "skip": {
      const [current, ...rest] = session.queue;
      if (current === undefined || rest.length === 0) return session;
      return { ...session, queue: [...rest, current] };
    }
    case "handled": {
      if (!session.queue.includes(event.threadId)) return session;
      return {
        ...session,
        queue: session.queue.filter((id) => id !== event.threadId),
        handled: [...session.handled, event.threadId],
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
  /** 1-based position of the one on screen; equals `total` when done. */
  position: number;
  total: number;
  done: number;
}

export function focusProgress(session: FocusSession): FocusProgress {
  const total = session.queue.length + session.handled.length;
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
