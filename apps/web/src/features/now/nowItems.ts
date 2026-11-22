import type { HiddenByMode } from "@/features/modes/modeDone";
import { formatTime } from "@/lib/format";

import type { Now, NowPerson, NowReadingPick, NowTodo, NowUpdatesCard } from "./api";

export type PersonItem = { kind: "person"; key: string; threadId: string; person: NowPerson };
export type TodoItem = { kind: "todo"; key: string; threadId: string | null; todo: NowTodo };
export type CardItem = { kind: "updates"; key: string; card: NowUpdatesCard };
export type PickItem = { kind: "reading"; key: string; threadId: string; pick: NowReadingPick };
export type NowItem = PersonItem | TodoItem | CardItem | PickItem;

export function nowItems(now: Now | undefined, hidden: HiddenByMode): NowItem[] {
  if (!now) return [];
  const items: NowItem[] = [];
  for (const person of now.people.rows) {
    const threadId = person.row.thread_id;
    if (hidden.messages.has(threadId)) continue;
    items.push({ kind: "person", key: `person:${threadId}`, threadId, person });
  }
  for (const todo of now.due_soon.todos) {
    const threadId = todo.todo.thread_id ?? null;
    if (threadId && hidden.todo.has(threadId)) continue;
    items.push({ kind: "todo", key: `todo:${todo.todo.id}`, threadId, todo });
  }
  const card = now.updates;
  if (
    card &&
    (card.thread_ids.length === 0 || !card.thread_ids.every((id) => hidden.updates.has(id)))
  ) {
    items.push({ kind: "updates", key: "updates", card });
  }
  if (now.reading) {
    const threadId = now.reading.thread_id;
    if (!hidden.reading.has(threadId)) {
      items.push({ kind: "reading", key: `reading:${threadId}`, threadId, pick: now.reading });
    }
  }
  return items;
}

export function litIndex(items: readonly { key: string }[], key: string | null): number {
  return Math.max(
    0,
    items.findIndex((item) => item.key === key),
  );
}

export interface ItemTarget {
  to: string;
  search?: { row: string };
}

export function itemTarget(item: NowItem, { email = false } = {}): ItemTarget {
  switch (item.kind) {
    case "person":
      return { to: `/messages/${encodeURIComponent(item.threadId)}` };
    case "todo": {
      const row = { row: item.todo.todo.id };
      return item.threadId
        ? { to: `/todo/${encodeURIComponent(item.threadId)}`, search: row }
        : { to: "/todo", search: row };
    }
    case "updates": {
      const first = item.card.lines?.[0];
      return first ? { to: "/updates", search: { row: first.id } } : { to: "/updates" };
    }
    case "reading": {
      if (email) return { to: `/reading/${encodeURIComponent(item.threadId)}` };
      const key = issueItemKey(item.pick.message_id);
      return { to: `/reading/item/${encodeURIComponent(key)}`, search: { row: key } };
    }
  }
}

function issueItemKey(messageId: string): string {
  return `${messageId}:0`;
}

export function sinceLabel(iso: string): string {
  const at = new Date(iso);
  const sameDay = at.toDateString() === new Date().toDateString();
  const day = sameDay ? "" : `${at.toLocaleDateString(undefined, { weekday: "short" })} `;
  return `since ${day}${formatTime(at)}`;
}

export function nowRowAccount(item: NowItem): string | undefined {
  switch (item.kind) {
    case "person":
      return item.person.row.account_id;
    case "todo":
      return item.todo.todo.account_id;
    case "reading":
      return item.pick.account_id;
    case "updates":
      return undefined;
  }
}
