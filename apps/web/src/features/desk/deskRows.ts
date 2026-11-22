import { DESK_LANES, type Desk, type DeskLaneKind, type DeskRow } from "./api";
import { LANE_TITLES } from "./deskCopy";
import {
  projectRows,
  type LensIdentity,
  type PendingMailOp,
} from "@/features/mail-actions/pendingMailOps";
import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";

export const DESK_LANE_CAP = 5;

export type DeskRowIndex = Map<string, DeskRow>;

export function toMessageRow(row: DeskRow): MessageRowView {
  const messageIds = row.message_ids.length > 0 ? row.message_ids : [row.message_id];
  return {
    id: row.message_id,
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

export function laneLens(lane: DeskLaneKind): LensIdentity {
  if (lane === "due") return { kind: "other" };
  return lane === "waiting" ? { kind: "desk-waiting" } : { kind: "desk" };
}

export const FOCUS_OWED_HREF = `/focus?lane=owed&from=${encodeURIComponent("/desk")}`;

export interface DeskGroups {
  groups: MessageGroupView[];
  index: DeskRowIndex;
  byThread: Map<string, DeskRow>;
}

export function deskGroups(
  desk: Desk | undefined,
  ops: readonly PendingMailOp[],
  only?: DeskLaneKind,
  hidden: ReadonlySet<string> = new Set(),
  basePath = "/desk",
): DeskGroups {
  const index: DeskRowIndex = new Map();
  const byThread = new Map<string, DeskRow>();
  if (!desk) return { groups: [], index, byThread };
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
    for (const row of data.rows) {
      index.set(row.message_id, row);
      byThread.set(row.thread_id, row);
    }
    groups.push({
      id: lane,
      label: LANE_TITLES[lane],
      rows: shown,
      count: total,
      more:
        !only && total > shown.length
          ? { label: `Show all ${total}`, href: `${basePath}?lane=${lane}` }
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
  return { groups, index, byThread };
}

export function partialLane(
  desk: Desk | undefined,
  lane: DeskLaneKind | undefined,
): { shown: number; total: number } | null {
  if (!desk || !lane) return null;
  const { rows, total } = desk[lane];
  return rows.length < total ? { shown: rows.length, total } : null;
}
