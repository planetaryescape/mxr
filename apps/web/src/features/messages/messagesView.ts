import { nextAfterRemoval } from "@/lib/listAdvance";

import type { ConversationMessage, MessagesData, MessagesRow, MessagesTopic } from "./api";

export type Band = "your_turn" | "pinned" | "recent" | "quiet";

export interface BandView {
  band: Band;
  title: string;
  rows: MessagesRow[];
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

export function cursorRows(data: MessagesData, quietOpen: boolean): MessagesRow[] {
  return [...data.your_turn, ...data.recent, ...(quietOpen ? data.quiet : [])];
}

export function rowForThread(data: MessagesData, threadId: string): MessagesRow | undefined {
  const all = [...data.your_turn, ...data.pinned, ...data.recent, ...data.quiet];
  return all.find((row) => row.topics.some((topic) => topic.thread_id === threadId));
}

export function waitLabel(since: string, now: Date = new Date()): string {
  const minutes = Math.max(0, Math.floor((now.getTime() - Date.parse(since)) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

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

export function rowTime(row: MessagesRow, now: Date = new Date()): string {
  return row.your_turn && row.turn_since
    ? waitLabel(row.turn_since, now)
    : dayLabel(row.last_at, now);
}

export function topicLabel(topic: MessagesTopic): string {
  const others = topic.with ?? [];
  return others.length > 0 ? `with ${others.join(", ")}: ${topic.subject}` : topic.subject;
}

const STATE_WORDS = {
  your_turn: "your turn",
  waiting: "waiting",
  done: "done here",
} satisfies Record<Exclude<MessagesTopic["state"], "quiet">, string>;

export function topicStateLabel(topic: MessagesTopic, now: Date = new Date()): string {
  const recency = waitLabel(topic.last_at, now);
  return topic.state === "quiet" ? recency : `${STATE_WORDS[topic.state]} · ${recency}`;
}

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

export function paragraphs(text: string): string[] {
  return paragraphBlocks(text).map((block) => block.text);
}

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

export function letterLead(message: ConversationMessage) {
  const blocks = paragraphs(message.text);
  if (message.source === "provider_snippet" || message.has_preserved_context) {
    return { lead: message.text, hidden: 0 };
  }
  if (message.layout === "compact" || blocks.length <= 1) {
    return { lead: message.text, hidden: 0 };
  }
  const ask = message.ask_quote;
  const asked = ask ? blocks.findIndex((block) => block.includes(ask)) : -1;
  if (asked >= 0) return { lead: blocks[asked] ?? "", hidden: blocks.length - 1 };
  const first = blocks[0] ?? "";
  const greeting = first.length <= GREETING_MAX_CHARS && first.endsWith(",") && blocks.length > 1;
  const shown = greeting ? 2 : 1;
  return { lead: blocks.slice(0, shown).join("\n\n"), hidden: blocks.length - shown };
}

const GREETING_MAX_CHARS = 30;

export interface TextPart {
  at: number;
  text: string;
  ask: boolean;
}

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

export function countdownLabel(endsAt: number, now: number): string {
  const seconds = Math.max(0, Math.ceil((endsAt - now) / 1000));
  return seconds > 0 ? `Sending in ${seconds}s` : "Sending…";
}

export function newTopicAddress(row: MessagesRow): string | null {
  return row.person?.id ?? null;
}

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

export function topicsLeft(
  topics: readonly MessagesTopic[],
  open: ReadonlySet<string>,
): MessagesTopic[] {
  return topics.filter((topic) => open.has(topic.thread_id));
}

export interface Arrival {
  key: number;
  person: string;
  thread: string | null;
}

export type DoneNext =
  | { kind: "topic"; topic: MessagesTopic }
  | { kind: "person"; row: MessagesRow }
  | { kind: "none" };

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
