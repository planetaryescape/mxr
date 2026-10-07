/*
 * Pure helpers for the Messages view: which rows the cursor walks, how a
 * row's time and a topic's state read, how a letter splits into what shows
 * first and what "Read all" reveals, where the ask sits in the text, and
 * how Got it's countdown reads. The daemon decides the bands, order and
 * words; nothing here reorders or rewrites them.
 */

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
export function dayLabel(at: string, now: Date = new Date()): string {
  const date = new Date(at);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(now) - startOf(date)) / 86_400_000);
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
  return row.your_turn && row.turn_since ? waitLabel(row.turn_since, now) : dayLabel(row.last_at, now);
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
  return text
    .split(/\n\s*\n/)
    .map((block) => block.trim())
    .filter(Boolean);
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
  const lead = (ask && blocks.find((block) => block.includes(ask))) || blocks[0] || "";
  return { lead, hidden: blocks.length - 1 };
}

export interface TextPart {
  text: string;
  ask: boolean;
}

/** The text split around the ask, so the ask can be highlighted. */
export function splitAsk(text: string, ask: string | null | undefined): TextPart[] {
  if (!ask) return [{ text, ask: false }];
  const at = text.indexOf(ask);
  if (at < 0) return [{ text, ask: false }];
  return [
    { text: text.slice(0, at), ask: false },
    { text: ask, ask: true },
    { text: text.slice(at + ask.length), ask: false },
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
