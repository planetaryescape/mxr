import { plural, startOfDay } from "@/lib/format";

import type { AiProvenance, ThreadContext, ThreadCounterparty } from "./api";

export function firstName(person: Pick<ThreadCounterparty, "display_name" | "email">): string {
  const name = person.display_name?.trim();
  if (!name) return person.email;
  return name.split(/\s+/)[0] ?? name;
}

export function shortDuration(seconds: number): string {
  const minutes = Math.max(1, Math.ceil(seconds / 60));
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 48 * 60) return `${Math.ceil(minutes / 60)}h`;
  return `${Math.ceil(minutes / (24 * 60))}d`;
}

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

export function contextFacts(context: ThreadContext, now = new Date()): string[] {
  const owed = owedReplyLabel(context, now);
  return [
    ...(context.counterparty ? relationshipParts(context.counterparty, now) : []),
    ...(owed ? [owed] : []),
  ];
}

export function owedReplyLabel(context: ThreadContext, now = new Date()): string | null {
  if (!context.owed_reply) return null;
  return `you owe a reply since ${dayLabel(context.owed_reply.since, now)}`;
}

export interface PromiseView {
  id: string;
  text: string;
}

export function promiseViews(context: ThreadContext, now = new Date()): PromiseView[] {
  return (context.promises ?? []).map(({ owner, commitment }) => {
    const who = ownerLabel(owner);
    const unverified = commitment.date_unverified ? " (date unverified)" : "";
    const due = commitment.by_when ? `, due ${dayLabel(commitment.by_when, now)}${unverified}` : "";
    return { id: commitment.id, text: `${who} promised: ${commitment.what}${due}` };
  });
}

export function provenanceScope(provenance: AiProvenance, personName: string | null): string {
  const withHistory = provenance.sources.includes("relationship_history");
  if (withHistory)
    return `from this thread and your history${personName ? ` with ${personName}` : ""}`;
  return provenance.locality === "local" ? "from this thread" : "from this thread only";
}

export function provenanceLabel(provenance: AiProvenance, personName: string | null): string {
  const place = provenance.locality === "local" ? "Local model" : "Cloud model";
  return `${place} ${provenance.model} · ${provenanceScope(provenance, personName)}`;
}

function ownerLabel(owner: string): string {
  if (owner.includes("@")) return owner;
  const first = firstName({ display_name: owner, email: owner });
  return first.charAt(0).toUpperCase() + first.slice(1);
}
