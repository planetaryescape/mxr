/*
 * Copy for the reader's context block. Pure functions over the daemon's
 * facts so the wording is testable: correct plurals, no em dashes, relative
 * when recent and absolute when older.
 */

import { plural } from "@/lib/format";

import type { AiProvenance, Commitment, ThreadContext, ThreadCounterparty } from "./api";

/** "Maya" from "Maya Ortiz"; the address when there is no name. */
export function firstName(person: Pick<ThreadCounterparty, "display_name" | "email">): string {
  const name = person.display_name?.trim();
  if (!name) return person.email;
  return name.split(/\s+/)[0] ?? name;
}

/** A median reply time at a glance: "35m", "4h", "2d". */
export function shortDuration(seconds: number): string {
  const minutes = Math.max(1, Math.ceil(seconds / 60));
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 48 * 60) return `${Math.ceil(minutes / 60)}h`;
  return `${Math.ceil(minutes / (24 * 60))}d`;
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/** "today", "yesterday", "Thu", "12 Sep", "12 Sep 2025". */
export function dayLabel(value: string, now = new Date()): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days > 1 && days < 7) return date.toLocaleDateString(undefined, { weekday: "short" });
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    ...(date.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  });
}

/**
 * How you know this person, in one line: "You and Maya: 41 emails · you
 * usually reply within 4h · last spoke 12 Sep". Parts with no data drop out.
 */
export function relationshipParts(person: ThreadCounterparty, now = new Date()): string[] {
  const name = firstName(person);
  const total = person.messages_from_them + person.messages_from_you;
  if (person.bulk_sender) {
    return [`Bulk mail from ${name}: ${plural(person.messages_from_them, "email")}`];
  }
  const parts = [`You and ${name}: ${plural(total, "email")}`];
  if (person.your_reply_p50_seconds != null) {
    parts.push(`you usually reply within ${shortDuration(person.your_reply_p50_seconds)}`);
  }
  if (person.their_reply_p50_seconds != null) {
    parts.push(`${name} usually replies within ${shortDuration(person.their_reply_p50_seconds)}`);
  }
  parts.push(
    person.last_contact_elsewhere_at
      ? `last spoke ${dayLabel(person.last_contact_elsewhere_at, now)}`
      : "your first conversation",
  );
  return parts;
}

export function owedReplyLabel(context: ThreadContext, now = new Date()): string | null {
  if (!context.owed_reply) return null;
  return `you owe a reply since ${dayLabel(context.owed_reply.since, now)}`;
}

export interface PromiseView {
  id: string;
  text: string;
}

/** "You promised: send the runbook link, due Fri" / "Maya promised: …". */
export function promiseViews(context: ThreadContext, now = new Date()): PromiseView[] {
  const theirName = context.counterparty ? firstName(context.counterparty) : null;
  return (context.commitments ?? []).map((commitment: Commitment) => {
    const who =
      commitment.direction === "yours" ? "You" : (theirName ?? capitalize(commitment.who_owes));
    const due = commitment.by_when ? `, due ${dayLabel(commitment.by_when, now)}` : "";
    return { id: commitment.id, text: `${who} promised: ${commitment.what}${due}` };
  });
}

/**
 * Where the model text came from: "Local model qwen2.5 · from this thread",
 * "Cloud model gpt-4o-mini · from this thread only".
 */
export function provenanceLabel(provenance: AiProvenance, personName: string | null): string {
  const place = provenance.locality === "local" ? "Local model" : "Cloud model";
  const withHistory = provenance.sources.includes("relationship_history");
  const source = withHistory
    ? `from this thread and your history${personName ? ` with ${personName}` : ""}`
    : provenance.locality === "local"
      ? "from this thread"
      : "from this thread only";
  return `${place} ${provenance.model} · ${source}`;
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
