import { formatTime } from "@/lib/format";

import type { ArrivalBucket, ArrivalCount, Arrivals } from "./types";

export type LinePart = { at: number } & ({ text: string } | { count: ArrivalCount });

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

export function linkedCounts(arrivals: Arrivals): ArrivalCount[] {
  return [...arrivals.counts, ...(arrivals.also ?? [])];
}

export interface CountLink {
  bucket: ArrivalBucket;
  since: string;
  until: string;
}

export function countLink(arrivals: Arrivals, count: ArrivalCount): CountLink {
  return { bucket: count.bucket, since: arrivals.since, until: arrivals.until };
}

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
  trash: "Trash",
  sorting: "Still sorting",
} as const satisfies Record<ArrivalBucket, string>;

export function bucketName(bucket: ArrivalBucket): string {
  return BUCKET_NAMES[bucket];
}

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
