/*
 * The arrivals protocol (crates/protocol/src/types/arrivals.rs), by hand.
 * The daemon's OpenAPI dump can't be regenerated while this branch waits for
 * the build queue; once `npm run gen:types` runs, these become aliases of
 * `components["schemas"]["ArrivalsData"]` and friends in `@/api/generated`.
 */

export type ModeId = "messages" | "todo" | "updates" | "reading" | "archive";

/** Where an arrival is counted, in the line's order. */
export const ARRIVAL_BUCKETS = [
  "messages",
  "todo",
  "updates",
  "reading",
  "archive",
  "screened_out",
  "spam",
  "sorting",
] as const;

export type ArrivalBucket = (typeof ARRIVAL_BUCKETS)[number];

export interface ArrivalCount {
  bucket: ArrivalBucket;
  count: number;
  /** "8 Messages". */
  label: string;
}

export interface MoveChoice {
  mode: ModeId;
  label: string;
  /** "m", "x", "u", "r", "e". */
  key: string;
}

export interface NotSure {
  account_id: string;
  message_id: string;
  thread_id: string;
  sender_email: string;
  sender_name?: string | null;
  subject: string;
  mode: ModeId;
  line: string;
  choices: MoveChoice[];
}

export interface Arrivals {
  generated_at: string;
  since: string;
  until: string;
  since_label: string;
  total: number;
  counts: ArrivalCount[];
  also?: ArrivalCount[];
  line: string;
  clear_line?: string | null;
  latest_at?: string | null;
  not_sure?: NotSure[];
  not_sure_line?: string | null;
  not_sure_hint?: string | null;
  track_record?: string | null;
  never_bury: string;
}

export interface ArrivalItem {
  account_id: string;
  message_id: string;
  thread_id: string;
  sender_email: string;
  sender_name?: string | null;
  subject: string;
  date: string;
  first_seen_at: string;
  arrived_in: ArrivalBucket;
  bucket: ArrivalBucket;
  reason?: string | null;
  /** "→ Updates · automated sender". */
  chip: string;
  moved?: boolean;
  not_sure?: boolean;
  unread?: boolean;
  also_todo?: boolean;
  also_archive?: boolean;
}

export interface ArrivalList {
  since: string;
  until: string;
  bucket?: ArrivalBucket | null;
  total: number;
  items: ArrivalItem[];
}

export interface MoveOutcome {
  account_id: string;
  message_id: string;
  thread_id: string;
  sender_email: string;
  from: ArrivalBucket;
  to: ModeId;
  sender: boolean;
  dry_run: boolean;
  copy: string;
  ask_sender?: string | null;
  hint?: string | null;
  correction_id?: number | null;
  aspect_id?: string | null;
}

export type ArrivalsResponse = { kind: "Arrivals"; arrivals: Arrivals };
export type ArrivalListResponse = { kind: "ArrivalList"; list: ArrivalList };
export type ArrivalModesResponse = { kind: "ArrivalModes"; items: ArrivalItem[] };
export type MessageMovedResponse = { kind: "MessageMoved"; outcome: MoveOutcome };
export type MoveUndoneResponse = { kind: "MoveUndone"; correction_id: number; copy: string };

/** Picker keys: a mode's `g` letter. Capitals set the sender's mode. */
export const MOVE_KEYS = {
  m: "messages",
  x: "todo",
  u: "updates",
  r: "reading",
  e: "archive",
} as const satisfies Record<string, ModeId>;

/** The mode a picker key names, if any (own keys only, never `toString`). */
export function modeForKey(key: string): ModeId | undefined {
  return Object.hasOwn(MOVE_KEYS, key) ? MOVE_KEYS[key as keyof typeof MOVE_KEYS] : undefined;
}

export const MODE_NAMES = {
  messages: "Messages",
  todo: "To do",
  updates: "Updates",
  reading: "Reading",
  archive: "Archive",
} as const satisfies Record<ModeId, string>;

/** Modes a sender's mail can be sent to; To do and Archive take one email. */
export const SENDER_MODES: readonly ModeId[] = ["messages", "updates", "reading"];
