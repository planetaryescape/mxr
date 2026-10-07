/*
 * Pure helpers for the Messages view: which rows the cursor walks, how a
 * row's time and a topic's state read, how a letter splits into what shows
 * first and what "Read all" reveals, where the ask sits in the text, and
 * how Got it's countdown reads. The daemon decides the bands, order and
 * words; nothing here reorders or rewrites them.
 */

import { nextAfterRemoval } from "@/lib/listAdvance";

import type { ConversationMessage, MessagesData, MessagesRow, MessagesTopic } from "./api";

/** Bands in the order they show. Pinned is a strip of faces, not rows. */
export type Band = "your_turn" | "pinned" | "recent" | "quiet";

export interface BandView {
  band: Band;
  title: string;
  rows: MessagesRow[];
  /** Rows the daemon holds beyond those listed. */
  more: number;
}

export function bands(data: MessagesData): BandView[] {
  return [
    { band: "your_turn" as const, title: "Your turn", rows: data.your_turn, more: 0 },
    { band: "pinned" as const, title: "Pinned", rows: data.pinned, more: 0 },
    {
      band: "recent" as const,
      title: "Recent",
      rows: data.recent,
      more: Math.max(0, data.recent_total - data.recent.length),
    },
    {
      band: "quiet" as const,
      title: "Quiet",
      rows: data.quiet,
      more: Math.max(0, data.quiet_total - data.quiet.length),
    },
  ].filter((view) => view.rows.length > 0);
}

/**
 * The rows j and k walk, top to bottom: Your turn, then Recent, then Quiet
 * once it's open. Pinned faces are reached by click; a pinned person whose
 * turn it is already sits in Your turn.
 */
export function cursorRows(data: MessagesData, quietOpen: boolean): MessagesRow[] {
  return [...data.your_turn, ...data.recent, ...(quietOpen ? data.quiet : [])];
}

/** The row holding a conversation, for links that name a thread. */
export function rowForThread(data: MessagesData, threadId: string): MessagesRow | undefined {
  const all = [...data.your_turn, ...data.pinned, ...data.recent, ...data.quiet];
  return all.find((row) => row.topics.some((topic) => topic.thread_id === threadId));
}

/** "16h", "3d", "now": how long Your turn has waited. */
export function waitLabel(since: string, now: Date = new Date()): string {
  const minutes = Math.max(0, Math.floor((now.getTime() - Date.parse(since)) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

/** "14:02", "Yesterday", "Tue", "12 Mar": when a quiet or recent row last moved. */
const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

export function dayLabel(at: string, now: Date = new Date()): string {
  const date = new Date(at);
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days <= 0) {
    return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  if (days === 1) return "Yesterday";
  if (days < 7) return date.toLocaleDateString(undefined, { weekday: "short" });
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    ...(date.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  });
}

/** The time a row shows: how long your turn has waited, else when it moved. */
export function rowTime(row: MessagesRow, now: Date = new Date()): string {
  return row.your_turn && row.turn_since
    ? waitLabel(row.turn_since, now)
    : dayLabel(row.last_at, now);
}

/** "AB" from "Ari Bell", "S" from "samir@…": the face in Pinned and rows. */
export function initials(title: string): string {
  const words = title
    .split(/[\s,]+/)
    .map((word) => word.replace(/[^\p{L}\p{N}]/gu, ""))
    .filter(Boolean);
  const letters = words.length > 1 ? [words[0], words[1]] : [words[0] ?? "?"];
  return letters
    .map((word) => (word ?? "?").charAt(0).toUpperCase())
    .join("")
    .slice(0, 2);
}

/** A topic's label on a person page: "with Ruth: Pricing copy" for a group. */
export function topicLabel(topic: MessagesTopic): string {
  const others = topic.with ?? [];
  return others.length > 0 ? `with ${others.join(", ")}: ${topic.subject}` : topic.subject;
}

const STATE_WORDS: Record<MessagesTopic["state"], string> = {
  your_turn: "your turn",
  waiting: "waiting",
  quiet: "quiet",
  done: "done here",
};

/** "your turn · 16h", "waiting · 1d". */
export function topicStateLabel(topic: MessagesTopic, now: Date = new Date()): string {
  return `${STATE_WORDS[topic.state]} · ${waitLabel(topic.last_at, now)}`;
}

/** The topic before or after `current`, wrapping at neither end. */
export function stepTopic(
  topics: readonly MessagesTopic[],
  current: string | null,
  delta: 1 | -1,
): MessagesTopic | undefined {
  if (topics.length === 0) return undefined;
  const at = topics.findIndex((topic) => topic.thread_id === current);
  if (at < 0) return topics[0];
  return topics[Math.min(topics.length - 1, Math.max(0, at + delta))];
}

/** A message's paragraphs, as the daemon's new text separates them. */
export function paragraphs(text: string): string[] {
  return paragraphBlocks(text).map((block) => block.text);
}

/** Paragraphs with where each starts in the text, a stable key. */
export function paragraphBlocks(text: string): { at: number; text: string }[] {
  const blocks: { at: number; text: string }[] = [];
  const separator = /\n\s*\n/g;
  let start = 0;
  for (const match of text.matchAll(separator)) {
    const block = text.slice(start, match.index).trim();
    if (block) blocks.push({ at: start, text: block });
    start = match.index + match[0].length;
  }
  const last = text.slice(start).trim();
  if (last) blocks.push({ at: start, text: last });
  return blocks;
}

/**
 * What a letter shows before "Read all": the paragraph holding the ask when
 * there is one, else the first. Short notes show whole.
 */
export function letterLead(message: ConversationMessage): { lead: string; hidden: number } {
  const blocks = paragraphs(message.text);
  if (message.layout === "compact" || blocks.length <= 1) {
    return { lead: message.text, hidden: 0 };
  }
  const ask = message.ask_quote;
  const asked = ask ? blocks.findIndex((block) => block.includes(ask)) : -1;
  if (asked >= 0) return { lead: blocks[asked] ?? "", hidden: blocks.length - 1 };
  // A greeting alone ("Hi Alex,") says nothing: lead with what follows too.
  const first = blocks[0] ?? "";
  const greeting = first.length <= GREETING_MAX_CHARS && first.endsWith(",") && blocks.length > 2;
  const shown = greeting ? 2 : 1;
  return { lead: blocks.slice(0, shown).join("\n\n"), hidden: blocks.length - shown };
}

/** A first paragraph this short ending in a comma is a greeting, not the point. */
const GREETING_MAX_CHARS = 30;

export interface TextPart {
  /** Where the part starts in the text: a stable key. */
  at: number;
  text: string;
  ask: boolean;
}

/** The text split around the ask, so the ask can be highlighted. */
export function splitAsk(text: string, ask: string | null | undefined): TextPart[] {
  if (!ask) return [{ at: 0, text, ask: false }];
  const at = text.indexOf(ask);
  if (at < 0) return [{ at: 0, text, ask: false }];
  return [
    { at: 0, text: text.slice(0, at), ask: false },
    { at, text: ask, ask: true },
    { at: at + ask.length, text: text.slice(at + ask.length), ask: false },
  ].filter((part) => part.text.length > 0);
}

/** "Sending in 4s": the Got it countdown, whole seconds, never below 0. */
export function countdownLabel(endsAt: number, now: number): string {
  const seconds = Math.max(0, Math.ceil((endsAt - now) / 1000));
  return seconds > 0 ? `Sending in ${seconds}s` : "Sending…";
}

/** Who a new topic goes to: the person's primary address. */
export function newTopicAddress(row: MessagesRow): string | null {
  return row.person?.id ?? null;
}

/**
 * A row's topics still in Messages: those not done here, less those whose
 * done is on its way to the daemon. What "left with them" counts, and
 * what keeps the row in view.
 */
export function openThreads(
  row: MessagesRow | undefined,
  hidden: ReadonlySet<string>,
): Set<string> {
  return new Set(
    (row?.topics ?? [])
      .filter((topic) => topic.state !== "done" && !hidden.has(topic.thread_id))
      .map((topic) => topic.thread_id),
  );
}

/** The topics on a person's page that are still in Messages. */
export function topicsLeft(
  topics: readonly MessagesTopic[],
  open: ReadonlySet<string>,
): MessagesTopic[] {
  return topics.filter((topic) => open.has(topic.thread_id));
}

/** A row or topic a verb just moved to: it glows once (`ArrivalGlow`). */
export interface Arrival {
  key: number;
  person: string;
  /** The topic it opened, or null when it opened a person. */
  thread: string | null;
}

/** What done here opens next. */
export type DoneNext =
  | { kind: "topic"; topic: MessagesTopic }
  | { kind: "person"; row: MessagesRow }
  | { kind: "none" };

/**
 * After done here on `thread`: the person's next topic still in Messages,
 * else the next person in the list (the previous one at the end), else
 * nothing. `rows` is the list as the cursor walks it; `hidden` holds the
 * threads whose done is on its way, so a person whose last topic is going
 * is never the one opened next.
 */
export function afterDone({
  topics,
  thread,
  open,
  rows,
  person,
  hidden = new Set(),
}: {
  topics: readonly MessagesTopic[];
  thread: string;
  open: ReadonlySet<string>;
  rows: readonly MessagesRow[];
  person: string;
  hidden?: ReadonlySet<string>;
}): DoneNext {
  const ids = topics.map((topic) => topic.thread_id);
  const nextThread =
    nextAfterRemoval(ids, thread, (id) => open.has(id)) ??
    ids.find((id) => id !== thread && open.has(id));
  const topic = topics.find((candidate) => candidate.thread_id === nextThread);
  if (topic) return { kind: "topic", topic };
  const rowIds = rows.map((row) => row.id);
  const going = new Set(
    rows
      .filter((row) => row.topics.length > 0 && openThreads(row, hidden).size === 0)
      .map((row) => row.id),
  );
  const nextRow =
    nextAfterRemoval(rowIds, person, (id) => !going.has(id)) ??
    rowIds.find((id) => id !== person && !going.has(id));
  const row = rows.find((candidate) => candidate.id === nextRow);
  return row ? { kind: "person", row } : { kind: "none" };
}

/**
 * Done here's toast: what was done, what opened next, then where the
 * daemon put it (its copy after the first sentence: "Archived in Gmail.",
 * "Still in To do."). "Done: Invoice. Next: Pricing. Archived in Gmail."
 */
export function doneLine(
  done: { subject: string } | { person: string },
  daemonCopy: string,
  next: DoneNext,
): string {
  const head = "subject" in done ? `Done: ${done.subject}.` : `Done with ${done.person}.`;
  const handoff = daemonCopy
    .split(/(?<=\.)\s+/)
    .slice(1)
    .join(" ");
  const tail =
    next.kind === "topic"
      ? `Next: ${topicLabel(next.topic)}.`
      : next.kind === "person"
        ? `Next: ${next.row.title}.`
        : "";
  return [head, tail, handoff].filter(Boolean).join(" ");
}
