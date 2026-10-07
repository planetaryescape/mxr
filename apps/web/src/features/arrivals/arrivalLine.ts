/*
 * The arrivals line as the daemon wrote it, cut into text and the counts
 * that link to their emails. The daemon owns every word; this only finds
 * each count's label in the line so it can be a link.
 */

import { formatTime } from "@/lib/format";

import type { ArrivalBucket, ArrivalCount, Arrivals } from "./types";

/** A piece of the line, with where it starts in the sentence (its key). */
export type LinePart = { at: number } & ({ text: string } | { count: ArrivalCount });

/**
 * Split `line` at each count's label, in order. A label the line doesn't
 * contain stays out of the links rather than breaking the sentence.
 */
export function lineParts(line: string, counts: readonly ArrivalCount[]): LinePart[] {
  const parts: LinePart[] = [];
  let from = 0;
  for (const count of counts) {
    const at = line.indexOf(count.label, from);
    if (at < 0) continue;
    if (at > from) parts.push({ at: from, text: line.slice(from, at) });
    parts.push({ at, count });
    from = at + count.label.length;
  }
  if (from < line.length) parts.push({ at: from, text: line.slice(from) });
  return parts;
}

/** The counts the line can link, in the order they appear in it. */
export function linkedCounts(arrivals: Arrivals): ArrivalCount[] {
  return [...arrivals.counts, ...(arrivals.also ?? [])];
}

/** What opening a count lists: that bucket, in the line's own window. */
export interface CountLink {
  bucket: ArrivalBucket;
  since: string;
  until: string;
}

export function countLink(arrivals: Arrivals, count: ArrivalCount): CountLink {
  return { bucket: count.bucket, since: arrivals.since, until: arrivals.until };
}

/**
 * Which sentence Now shows: the clear line once nothing on Now waits and
 * every arrival is accounted for, else the counts.
 */
export function shownLine(arrivals: Arrivals, nowWaiting: boolean): string {
  return !nowWaiting && arrivals.clear_line ? arrivals.clear_line : arrivals.line;
}

const BUCKET_NAMES = {
  messages: "Messages",
  todo: "To do",
  updates: "Updates",
  reading: "Reading",
  archive: "Archive",
  screened_out: "Screened out",
  spam: "Spam",
  sorting: "Still sorting",
} as const satisfies Record<ArrivalBucket, string>;

export function bucketName(bucket: ArrivalBucket): string {
  return BUCKET_NAMES[bucket];
}

/** "8 in Reading since 08:12", for the list page's heading. */
export function listTitle(
  total: number,
  bucket: ArrivalBucket | undefined,
  since?: string,
): string {
  const emails = total === 1 ? "1 email" : `${total} emails`;
  const where = bucket ? ` in ${bucketName(bucket)}` : "";
  const when = since ? ` since ${clock(since)}` : "";
  return `${emails}${where}${when}`;
}

function clock(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  const sameDay = at.toDateString() === new Date().toDateString();
  const day = sameDay ? "" : `${at.toLocaleDateString(undefined, { weekday: "short" })} `;
  return `${day}${formatTime(at)}`;
}
