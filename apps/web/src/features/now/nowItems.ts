/*
 * Now's rows in their fixed order (People, Due soon, the Updates card, the
 * evening Reading pick), as one list the keyboard walks. The daemon has
 * already capped each section; this only flattens them and leaves out
 * rows whose done is in flight.
 */

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
  if (card && !card.thread_ids.every((id) => hidden.updates.has(id))) {
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

/** Where Enter takes an item: into its own mode. */
export function itemPath(item: NowItem): string {
  switch (item.kind) {
    case "person":
      return `/messages/${encodeURIComponent(item.threadId)}`;
    case "todo":
      return item.threadId ? `/todo/${encodeURIComponent(item.threadId)}` : "/todo";
    case "updates":
      return "/updates";
    case "reading":
      return `/reading/${encodeURIComponent(item.threadId)}`;
  }
}

/** "since 4:30 PM", or "since Fri 8:00 AM" before today, as times read everywhere. */
export function sinceLabel(iso: string): string {
  const at = new Date(iso);
  const sameDay = at.toDateString() === new Date().toDateString();
  const day = sameDay ? "" : `${at.toLocaleDateString(undefined, { weekday: "short" })} `;
  return `since ${day}${formatTime(at)}`;
}
