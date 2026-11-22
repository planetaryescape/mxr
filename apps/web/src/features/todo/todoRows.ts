import { firstName } from "@/features/thread/context/contextFormat";

import type { Todo, TodoField, TodoRunway } from "./api";

export type PrimaryAction =
  | { kind: "email"; label: string; url: string; domain?: string; messageId?: string }
  | { kind: "reply"; label: string; messageId: string }
  | { kind: "open"; label: string };

export function primaryAction(todo: Todo): PrimaryAction | null {
  const action = todo.action;
  if (action) {
    return {
      kind: "email",
      label: action.label,
      url: action.url,
      domain: action.domain ?? undefined,
      messageId: action.message_id ?? todo.source_message_id ?? undefined,
    };
  }
  if (todo.kind === "promise" && todo.source_message_id) {
    const who = todo.counterparty
      ? firstName({ display_name: todo.counterparty, email: "" })
      : null;
    return {
      kind: "reply",
      label: who ? `Reply to ${who}` : "Reply",
      messageId: todo.source_message_id,
    };
  }
  if (!todo.thread_id) return null;
  return { kind: "open", label: todo.kind === "rsvp" ? "Open the invite" : "Open email" };
}

export function rowParty(todo: Todo): string | null {
  return todo.person_label ?? todo.counterparty ?? null;
}

const SHORT_DAY = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const SHORT_DATE = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const SHORT_DATE_YEAR = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  year: "numeric",
});

const midnight = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate());

export function arrivedDay(arrived: Date, now: Date): string {
  const days = Math.round((midnight(now).getTime() - midnight(arrived).getTime()) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 7) return SHORT_DAY.format(arrived);
  return arrived.getFullYear() === now.getFullYear()
    ? SHORT_DATE.format(arrived)
    : SHORT_DATE_YEAR.format(arrived);
}

export function rowSecondary(todo: Todo, now: Date): string | null {
  const who = rowParty(todo);
  const arrival =
    todo.kind === "promise"
      ? "from you"
      : todo.source_date
        ? `arrived ${arrivedDay(new Date(todo.source_date), now)}`
        : null;
  const filed = todo.source_filed_at
    ? `left the inbox ${arrivedDay(new Date(todo.source_filed_at), now)}`
    : null;
  const notice =
    todo.resolution?.status === "pending"
      ? "a later email may change this"
      : todo.looks_done
        ? `looks done: ${todo.looks_done.reason}`
        : null;
  return [who, arrival, filed, notice].filter(Boolean).join(" · ") || null;
}

export function dueByToday(todo: Todo, now: Date): boolean {
  if (todo.overdue) return true;
  const times = [todo.due_at, todo.act_by_at]
    .filter((value): value is string => Boolean(value))
    .map((value) => new Date(value).getTime());
  if (times.length === 0) return false;
  const tomorrow = new Date(now);
  tomorrow.setHours(24, 0, 0, 0);
  return Math.min(...times) < tomorrow.getTime();
}

const FIELD_LABELS: Record<string, string> = {
  title: "What",
  kind: "Kind",
  counterparty: "Who",
  amount: "Amount",
  due_at: "Due",
  act_by_at: "Act by",
  surface_at: "Shows up",
  relevant_until: "Lets go",
  action_url: "Link",
  event_start: "Event",
};

const FIELD_ORDER = Object.keys(FIELD_LABELS);

export function fieldLabel(field: string): string {
  return FIELD_LABELS[field] ?? field.replaceAll("_", " ");
}

export function orderedFields(fields: readonly TodoField[]): TodoField[] {
  const rank = (field: string) => {
    const index = FIELD_ORDER.indexOf(field);
    return index === -1 ? FIELD_ORDER.length : index;
  };
  return fields.toSorted((a, b) => rank(a.field) - rank(b.field));
}

export function provenanceSummary(fields: readonly TodoField[]): {
  source: string;
  unchecked: number;
} {
  const counts = new Map<string, number>();
  for (const field of fields) {
    if (field.source === "table") continue;
    counts.set(field.source, (counts.get(field.source) ?? 0) + 1);
  }
  const main = [...counts.entries()].toSorted((a, b) => b[1] - a[1])[0]?.[0] ?? "table";
  return {
    source: SOURCE_SHORT[main] ?? main,
    unchecked: fields.filter((field) => !field.checked).length,
  };
}

const SOURCE_SHORT: Record<string, string> = {
  schema: "schema.org",
  ics: "the invite",
  rule: "a rule",
  table: "the lead-time table",
  model: "a model",
  user: "you",
};

export type Band = "now" | "coming" | "whenever" | "done";

export interface RunwayItem {
  todo: Todo;
  band: Band;
  group?: string;
}

export function runwayItems(
  runway: TodoRunway,
  open: { whenever: boolean; done: boolean },
  hidden: ReadonlySet<string> = new Set(),
): RunwayItem[] {
  const items: RunwayItem[] = [];
  const push = (todos: readonly Todo[], band: Band, group?: string) => {
    for (const todo of todos) if (!hidden.has(todo.id)) items.push({ todo, band, group });
  };
  push(runway.now, "now");
  for (const week of runway.coming_up) push(week.todos, "coming", week.label);
  push(runway.later, "coming", "Later");
  if (open.whenever) push(runway.whenever, "whenever");
  if (open.done) push(runway.done_this_week, "done");
  return items;
}

export function openCount(runway: TodoRunway): number {
  return (
    runway.now.length +
    runway.coming_up.reduce((sum, week) => sum + week.todos.length, 0) +
    runway.later.length +
    runway.whenever.length
  );
}

export function linkLine(todo: Todo): string | null {
  const domain = todo.action?.domain;
  return domain ? `The link in the email goes to ${domain}.` : null;
}
