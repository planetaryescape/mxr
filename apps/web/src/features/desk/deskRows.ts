/*
 * Desk rows as list rows. The desk renders through the shared mail list so
 * the cursor, the triage verbs, optimistic removal and undo behave exactly
 * as they do in every other list.
 */

import { DESK_LANES, type Desk, type DeskLaneKind, type DeskRow } from "./api";
import { LANE_TITLES } from "./deskCopy";
import {
  projectRows,
  type LensIdentity,
  type PendingMailOp,
} from "@/features/mail-actions/pendingMailOps";
import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";

/** Rows each lane shows on the desk itself; the rest are one link away. */
export const DESK_LANE_CAP = 5;

/** A list row plus the desk row it came from, keyed by the list row's id. */
export type DeskRowIndex = Map<string, DeskRow>;

export function toMessageRow(row: DeskRow): MessageRowView {
  const messageIds = row.message_ids.length > 0 ? row.message_ids : [row.message_id];
  return {
    id: row.message_id,
    // Thread rows act on the whole conversation: archiving a desk row
    // clears every message, so the row cannot come back on refetch.
    kind: messageIds.length > 1 ? "thread" : "message",
    message_ids: messageIds,
    message_count: messageIds.length,
    account_id: row.account_id,
    thread_id: row.thread_id,
    provider_id: "",
    sender: row.counterparty_name?.trim() || row.counterparty_email,
    sender_detail: row.counterparty_email,
    subject: row.subject,
    snippet: row.reason,
    date: row.since,
    date_label: "",
    date_full: row.since,
    date_relative: "",
    unread: Boolean(row.unread),
    starred: Boolean(row.starred),
    has_attachments: false,
  };
}

/**
 * Archive, snooze, trash and moves take owed and new-from-people rows off
 * the desk, as they leave the inbox. A promise stays until it is resolved,
 * whatever happens to its thread, so due rows only leave for trash and spam.
 * Waiting rows follow the daemon: snooze, trash and spam set them aside,
 * and archive never reaches them (the desk turns it into done waiting).
 */
export function laneLens(lane: DeskLaneKind): LensIdentity {
  if (lane === "due") return { kind: "other" };
  return lane === "waiting" ? { kind: "desk-waiting" } : { kind: "desk" };
}

/** Focus mode on the owed lane, returning to the desk on Esc. */
export const FOCUS_OWED_HREF = `/focus?lane=owed&from=${encodeURIComponent("/desk")}`;

export interface DeskGroups {
  groups: MessageGroupView[];
  index: DeskRowIndex;
}

/**
 * Lanes as list groups, with pending operations applied. `only` shows one
 * lane in full (a lane's own page); otherwise each lane shows its first
 * few rows and its remaining total. Counts drop as rows leave optimistically.
 */
export function deskGroups(
  desk: Desk | undefined,
  ops: readonly PendingMailOp[],
  only?: DeskLaneKind,
  /** Conversations leaving optimistically ("done waiting" in flight). */
  hidden: ReadonlySet<string> = new Set(),
): DeskGroups {
  const index: DeskRowIndex = new Map();
  if (!desk) return { groups: [], index };
  const groups: MessageGroupView[] = [];
  for (const lane of DESK_LANES) {
    if (only && lane !== only) continue;
    const data = desk[lane];
    const rows = data.rows.map(toMessageRow);
    const visible = projectRows(rows, ops, laneLens(lane)).filter(
      (row) => !hidden.has(row.thread_id),
    );
    const total = Math.max(0, data.total - (rows.length - visible.length));
    if (visible.length === 0) continue;
    const shown = only ? visible : visible.slice(0, DESK_LANE_CAP);
    for (const row of data.rows) index.set(row.message_id, row);
    groups.push({
      id: lane,
      label: LANE_TITLES[lane],
      rows: shown,
      count: total,
      more:
        !only && total > shown.length
          ? { label: `Show all ${total}`, href: `/desk?lane=${lane}` }
          : undefined,
      action:
        lane === "owed"
          ? {
              label: `Reply to ${total === 1 ? "them" : `all ${total}`} in focus mode`,
              href: FOCUS_OWED_HREF,
              keys: "g F",
            }
          : undefined,
    });
  }
  return { groups, index };
}

/**
 * On a lane's page, how much of the lane the daemon sent, when that is
 * less than all of it ("Showing 100 of 143").
 */
export function partialLane(
  desk: Desk | undefined,
  lane: DeskLaneKind | undefined,
): { shown: number; total: number } | null {
  if (!desk || !lane) return null;
  const { rows, total } = desk[lane];
  return rows.length < total ? { shown: rows.length, total } : null;
}
