/*
 * Optimistic mail state as data. Every in-flight mutation is a pending
 * operation; views render the server's rows with those operations projected
 * on top. A failed operation is simply removed, so it can't roll back a
 * later action's effect, and a refetch mid-flight can't make the row flicker
 * back: the projection re-applies to whatever the server returns.
 */

import { useMemo } from "react";
import { create } from "zustand";

import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";

export type MailAction =
  | "archive"
  | "trash"
  | "spam"
  | "star"
  | "unstar"
  | "read"
  | "unread"
  | "read-and-archive"
  | "move"
  | "route"
  | "label-add"
  | "label-remove"
  | "labels"
  | "snooze";

export interface MailActionPayload {
  /** Target label for label-add, label-remove, move and route. */
  label?: string;
  /** "labels": several changes in one request. */
  add?: string[];
  remove?: string[];
  /** Route: queue label being cleared, and whether to archive. */
  fromQueueLabel?: string;
  archive?: boolean;
}

export interface PendingMailOp {
  id: string;
  action: MailAction;
  messageIds: ReadonlySet<string>;
  payload?: MailActionPayload;
}

/**
 * What a view is showing, so a projection only removes rows the action
 * really takes out of it. Archive leaves All Mail untouched; removing a
 * label only hides rows in that label's lens.
 */
export type LensIdentity =
  | { kind: "inbox" }
  | { kind: "all_mail" }
  | { kind: "starred" }
  | { kind: "trash" }
  | { kind: "spam" }
  | { kind: "label"; labelName: string }
  | { kind: "search" }
  | { kind: "other" };

interface PendingMailOpsState {
  ops: PendingMailOp[];
  add: (op: PendingMailOp) => void;
  remove: (id: string) => void;
}

export const usePendingMailOps = create<PendingMailOpsState>((set) => ({
  ops: [],
  add: (op) => set((state) => ({ ops: [...state.ops, op] })),
  remove: (id) => set((state) => ({ ops: state.ops.filter((op) => op.id !== id) })),
}));

/** Every message a row stands for: all of a thread's messages, or itself. */
export function rowMessageIds(row: Pick<MessageRowView, "id" | "message_ids">): string[] {
  return row.message_ids && row.message_ids.length > 0 ? row.message_ids : [row.id];
}

function touches(row: MessageRowView, op: PendingMailOp): boolean {
  return rowMessageIds(row).some((id) => op.messageIds.has(id));
}

function sameLabel(a: string, b: string): boolean {
  return a.localeCompare(b, undefined, { sensitivity: "accent" }) === 0;
}

/** True when `op` takes the row out of the lens being shown. */
export function removesFromLens(op: PendingMailOp, lens: LensIdentity): boolean {
  switch (op.action) {
    case "archive":
    case "read-and-archive":
    case "snooze":
      return lens.kind === "inbox";
    case "trash":
      return lens.kind !== "trash";
    case "spam":
      return lens.kind !== "spam";
    case "move":
      return (
        lens.kind === "inbox" ||
        (lens.kind === "label" &&
          !!op.payload?.label &&
          !sameLabel(lens.labelName, op.payload.label))
      );
    case "route":
      return (
        (lens.kind === "inbox" && op.payload?.archive !== false) ||
        (lens.kind === "label" &&
          !!op.payload?.fromQueueLabel &&
          sameLabel(lens.labelName, op.payload.fromQueueLabel))
      );
    case "label-remove":
      return (
        lens.kind === "label" && !!op.payload?.label && sameLabel(lens.labelName, op.payload.label)
      );
    case "labels":
      return (
        lens.kind === "label" &&
        (op.payload?.remove ?? []).some((name) => sameLabel(lens.labelName, name))
      );
    case "unstar":
      return lens.kind === "starred";
    default:
      return false;
  }
}

function applyFlags(row: MessageRowView, op: PendingMailOp): MessageRowView {
  switch (op.action) {
    case "star":
      return row.starred ? row : { ...row, starred: true };
    case "unstar":
      return row.starred ? { ...row, starred: false } : row;
    case "read":
    case "read-and-archive":
      return row.unread ? { ...row, unread: false } : row;
    case "unread":
      return row.unread ? row : { ...row, unread: true };
    default:
      return row;
  }
}

export function projectRows(
  rows: MessageRowView[],
  ops: readonly PendingMailOp[],
  lens: LensIdentity,
): MessageRowView[] {
  if (ops.length === 0) return rows;
  const out: MessageRowView[] = [];
  for (const original of rows) {
    let row: MessageRowView | null = original;
    for (const op of ops) {
      if (!row || !touches(row, op)) continue;
      row = removesFromLens(op, lens) ? null : applyFlags(row, op);
    }
    if (row) out.push(row);
  }
  return out;
}

export function projectGroups(
  groups: MessageGroupView[],
  ops: readonly PendingMailOp[],
  lens: LensIdentity,
): MessageGroupView[] {
  if (ops.length === 0) return groups;
  return groups
    .map((group) => ({ ...group, rows: projectRows(group.rows, ops, lens) }))
    .filter((group) => group.rows.length > 0);
}

/** Server groups with every pending operation applied, for rendering. */
export function useProjectedGroups(
  groups: MessageGroupView[] | undefined,
  lens: LensIdentity,
): MessageGroupView[] {
  const ops = usePendingMailOps((state) => state.ops);
  const lensKey = lens.kind === "label" ? `label:${lens.labelName}` : lens.kind;
  // eslint-disable-next-line react-hooks/exhaustive-deps -- lens is keyed by lensKey
  return useMemo(() => projectGroups(groups ?? [], ops, lens), [groups, ops, lensKey]);
}

/** Thread messages with flag changes (star, read) applied. */
export function useProjectedMessages(messages: MessageRowView[]): MessageRowView[] {
  const ops = usePendingMailOps((state) => state.ops);
  return useMemo(() => {
    if (ops.length === 0) return messages;
    return messages.map((message) =>
      ops.reduce((row, op) => (op.messageIds.has(row.id) ? applyFlags(row, op) : row), message),
    );
  }, [messages, ops]);
}
