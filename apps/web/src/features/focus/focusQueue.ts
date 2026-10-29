/*
 * The focus & reply queue: everyone you owe a reply, one conversation per
 * entry. The desk's You owe lane comes first in its order (people only, in
 * the inbox, people you have written to, furthest past your usual pace
 * first), then the reply-later queue in its order, skipping conversations
 * already listed.
 *
 * The session is a pure reducer so its rules are tested without React:
 * the order is fixed when you start, so a refetch never reshuffles what's
 * next; new arrivals join the end; handled conversations never come back
 * unless a send is undone; the one on screen stays put even if a refetch
 * drops it (you may be halfway through the reply).
 */

import type { DeskRow } from "@/features/desk/api";
import { rowAge } from "@/features/desk/deskCopy";
import { conversationKey } from "@/features/mailbox/rowKey";
import type { ReplyQueueMessage } from "@/features/reply-queue/api";

export interface FocusItem {
  threadId: string;
  /**
   * The account the conversation is in. A thread id can be held by two
   * accounts, so with it the queue tells the two apart (`focusKey`).
   */
  accountId?: string;
  /** The message a reply answers: the newest from them. */
  messageId: string;
  sender: string;
  subject: string;
  /** Why it is here, in the daemon's words. */
  reason: string;
  source: "owed" | "reply-later";
}

/**
 * What names a conversation in the session: its thread within its account,
 * or the bare thread id for data that has no account.
 */
export function focusKey(item: Pick<FocusItem, "threadId" | "accountId">): string {
  return conversationKey(item.threadId, item.accountId);
}

export function buildFocusQueue(
  owed: readonly DeskRow[],
  replyLater: readonly ReplyQueueMessage[],
  accountScope: string | null,
): FocusItem[] {
  const items: FocusItem[] = [];
  const seen = new Set<string>();
  // Threads listed by an account-less entry: another entry of the thread would be the same one.
  const seenThreads = new Set<string>();
  for (const row of owed) {
    const key = focusKey({ threadId: row.thread_id, accountId: row.account_id });
    if (seen.has(key)) continue;
    seen.add(key);
    seenThreads.add(row.thread_id);
    items.push({
      threadId: row.thread_id,
      accountId: row.account_id,
      messageId: row.message_id,
      sender: row.counterparty_name?.trim() || row.counterparty_email,
      subject: row.subject,
      reason: owedReason(row),
      source: "owed",
    });
  }
  for (const message of replyLater) {
    // The reply-later list isn't account scoped on the wire; scope it here.
    if (accountScope && message.account_id && message.account_id !== accountScope) continue;
    const key = focusKey({ threadId: message.thread_id, accountId: message.account_id });
    if (seen.has(key) || (!message.account_id && seenThreads.has(message.thread_id))) continue;
    seen.add(key);
    items.push({
      threadId: message.thread_id,
      accountId: message.account_id,
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
  /** Every conversation seen this session, by `focusKey`. */
  items: Record<string, FocusItem>;
  /** Still to do, in order; the first is on screen. */
  queue: string[];
  /** Sent (or in its undo window) or snoozed, in the order handled. */
  handled: string[];
  /** Skipped when nothing else was left: set aside until you come back. */
  deferred: string[];
  /** The source has answered at least once. */
  loaded: boolean;
}

export type FocusEvent =
  | { kind: "sync"; items: FocusItem[] }
  | { kind: "skip" }
  | { kind: "handled"; key: string }
  | { kind: "restore"; key: string }
  /** Put a queued conversation on screen: a link that named it. */
  | { kind: "front"; key: string }
  /** Bring the set-aside conversations back into the queue. */
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
        items[focusKey(item)] = item;
        inSource.add(focusKey(item));
      }
      const current = session.queue[0];
      const kept = session.queue.filter((id) => id === current || inSource.has(id));
      const known = new Set([...kept, ...session.handled, ...session.deferred]);
      const arrivals = event.items.map(focusKey).filter((id) => !known.has(id));
      const queue = [...kept, ...arrivals];
      // A refetch that changes nothing keeps the same session, so the page
      // and the conversation on screen don't re-render.
      if (
        session.loaded &&
        sameIds(queue, session.queue) &&
        event.items.every((item) => sameItem(session.items[focusKey(item)], item))
      ) {
        return session;
      }
      return { ...session, items, queue, loaded: true };
    }
    case "skip": {
      const [current, ...rest] = session.queue;
      if (current === undefined) return session;
      // The last one left is set aside rather than shown again.
      if (rest.length === 0)
        return { ...session, queue: [], deferred: [...session.deferred, current] };
      return { ...session, queue: [...rest, current] };
    }
    case "revisit": {
      if (session.deferred.length === 0) return session;
      return { ...session, queue: [...session.queue, ...session.deferred], deferred: [] };
    }
    case "handled": {
      if (!session.queue.includes(event.key)) return session;
      return {
        ...session,
        queue: session.queue.filter((id) => id !== event.key),
        handled: [...session.handled, event.key],
      };
    }
    case "front": {
      if (!session.queue.includes(event.key) || session.queue[0] === event.key) return session;
      return { ...session, queue: [event.key, ...session.queue.filter((id) => id !== event.key)] };
    }
    case "restore": {
      if (!session.handled.includes(event.key)) return session;
      return {
        ...session,
        queue: [event.key, ...session.queue],
        handled: session.handled.filter((id) => id !== event.key),
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

/** "Replied to your message · 2d · usually 4h", as the desk row reads. */
function owedReason(row: DeskRow): string {
  const age = rowAge(row);
  const parts = [row.reason.charAt(0).toUpperCase() + row.reason.slice(1), age.label, age.usual];
  return parts.filter(Boolean).join(" · ");
}
