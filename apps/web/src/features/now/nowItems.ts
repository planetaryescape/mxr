/*
 * Now's rows in their fixed order (People, Due soon, the Updates card, the
 * evening Reading pick), as one list the keyboard walks. The daemon has
 * already capped each section; this only flattens them and leaves out
 * rows whose done is in flight.
 */

import { hiddenKey } from "@/features/modes/modeDone";

import type { Now, NowPerson, NowReadingPick, NowTodo, NowUpdatesCard } from "./api";

export type NowItem =
  | { kind: "person"; key: string; threadId: string; person: NowPerson }
  | { kind: "todo"; key: string; threadId: string | null; todo: NowTodo }
  | { kind: "updates"; key: string; card: NowUpdatesCard }
  | { kind: "reading"; key: string; threadId: string; pick: NowReadingPick };

export function nowItems(now: Now | undefined, hidden: ReadonlySet<string>): NowItem[] {
  if (!now) return [];
  const items: NowItem[] = [];
  for (const person of now.people.rows) {
    const threadId = person.row.thread_id;
    if (hidden.has(hiddenKey("messages", threadId))) continue;
    items.push({ kind: "person", key: `person:${threadId}`, threadId, person });
  }
  for (const todo of now.due_soon.todos) {
    const threadId = todo.todo.thread_id ?? null;
    if (threadId && hidden.has(hiddenKey("todo", threadId))) continue;
    items.push({ kind: "todo", key: `todo:${todo.todo.id}`, threadId, todo });
  }
  const card = now.updates;
  if (card && !card.thread_ids.every((id) => hidden.has(hiddenKey("updates", id)))) {
    items.push({ kind: "updates", key: "updates", card });
  }
  if (now.reading) {
    const threadId = now.reading.thread_id;
    if (!hidden.has(hiddenKey("reading", threadId))) {
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

/** "since 16:30", in the viewer's clock. */
export function sinceLabel(iso: string, locale?: string): string {
  const at = new Date(iso);
  const time = at.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", hour12: false });
  const today = new Date();
  const sameDay = at.toDateString() === today.toDateString();
  return sameDay
    ? `since ${time}`
    : `since ${at.toLocaleDateString(locale, { weekday: "short" })} ${time}`;
}
