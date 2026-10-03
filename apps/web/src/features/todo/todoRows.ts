/*
 * What a To do row shows and what Enter does, as plain data so it can be
 * tested without the DOM. The daemon owns the words (title, when label,
 * why line, the button's label); this decides which of them a row uses.
 */

import {
  Banknote,
  CalendarCheck,
  CircleDot,
  CreditCard,
  FileText,
  Handshake,
  House,
  PenLine,
  RefreshCw,
  ShieldCheck,
  Undo2,
  type LucideIcon,
} from "lucide-react";

import { firstName } from "@/features/thread/context/contextFormat";

import type { Todo, TodoField, TodoRunway } from "./api";

/**
 * What Enter (and the row's one button) does. Nothing on a to-do opens an
 * outside link: a row about a link opens its email in mxr with that link
 * marked, so you read it in context before you choose to follow it.
 */
export type PrimaryAction =
  /** Open the email with the link the row is about marked. */
  | { kind: "email"; label: string; url: string; domain?: string; messageId?: string }
  /** A promise you made: reply in the conversation. */
  | { kind: "reply"; label: string; messageId: string }
  /** Nothing to point at: open the conversation. */
  | { kind: "open"; label: string };

/** The row's one action, labelled with its verb: "Open email to pay". */
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

/** Who the row is for: "you promised Priya" on a promise, else the counterparty. */
export function rowParty(todo: Todo): string | null {
  return todo.person_label ?? todo.counterparty ?? null;
}

/**
 * How full the runway bar is, from the day it showed up (0) to the
 * deadline (1). Past the deadline there is no bar: the date reads "was
 * due Fri" instead.
 */
export function runwayFill(todo: Todo): number | null {
  if (todo.overdue || todo.runway === null || todo.runway === undefined) return null;
  return Math.min(1, Math.max(0, todo.runway));
}

const KIND_ICONS: Record<string, LucideIcon> = {
  bill: Banknote,
  payment_failed: CreditCard,
  renewal: RefreshCw,
  document: FileText,
  lease: House,
  return: Undo2,
  rsvp: CalendarCheck,
  verify: ShieldCheck,
  sign: PenLine,
  promise: Handshake,
};

export function kindIcon(kind: string): LucideIcon {
  return KIND_ICONS[kind] ?? CircleDot;
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

/** Fields in the order the provenance list shows them. */
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

/**
 * The chip's short form: the main source ("schema.org", "a rule", "you")
 * and how many fields still need a check (an open dot each).
 */
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

/** One row the cursor can rest on, in screen order. */
export interface RunwayItem {
  todo: Todo;
  band: Band;
  /** The week or "Later" heading it sits under in Coming up. */
  group?: string;
}

/**
 * The runway's rows in screen order: Now, Coming up by week then Later,
 * and Whenever and Done this week only while they are opened.
 */
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

/** Rows on the runway that still need doing, for "has items" checks. */
export function openCount(runway: TodoRunway): number {
  return (
    runway.now.length +
    runway.coming_up.reduce((sum, week) => sum + week.todos.length, 0) +
    runway.later.length +
    runway.whenever.length
  );
}

/** Where the row's link goes, said before Enter: "The link goes to camden.gov.uk." */
export function linkLine(todo: Todo): string | null {
  const domain = todo.action?.domain;
  return domain ? `The link in the email goes to ${domain}.` : null;
}
