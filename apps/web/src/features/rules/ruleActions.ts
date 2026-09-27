/*
 * Apply-now for the rule builder: parses the rule's action string into the
 * mailbox mutations the web can run, and runs them in order on the exact
 * message ids the preview listed.
 */

import {
  archiveMessages,
  markReadMessages,
  modifyLabels,
  moveMessagesToLabel,
  readAndArchiveMessages,
  spamMessages,
  starMessages,
  trashMessages,
} from "@/features/mailbox/api";
import type { MutationResponse } from "@/features/mailbox/types";

export type SupportedRuleAction =
  | { kind: "archive" }
  | { kind: "trash" }
  | { kind: "spam" }
  | { kind: "star" }
  | { kind: "read" }
  | { kind: "unread" }
  | { kind: "read-and-archive" }
  | { kind: "label-add"; label: string }
  | { kind: "label-remove"; label: string }
  | { kind: "move"; label: string };

export function mailActions(value: string): SupportedRuleAction[] | null {
  const actions = value
    .split(/[;,]/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map(mailAction);
  if (actions.length === 0 || actions.some((action) => action === null)) return null;
  return actions as SupportedRuleAction[];
}

function mailAction(value: string): SupportedRuleAction | null {
  const normalized = value.trim();
  const lower = normalized.toLowerCase();
  if (lower === "archive") return { kind: "archive" };
  if (lower === "trash") return { kind: "trash" };
  if (lower === "spam") return { kind: "spam" };
  if (lower === "star") return { kind: "star" };
  if (lower === "read" || lower === "mark-read" || lower === "mark_read") return { kind: "read" };
  if (lower === "unread" || lower === "mark-unread" || lower === "mark_unread")
    return { kind: "unread" };
  if (lower === "read-and-archive" || lower === "read_and_archive")
    return { kind: "read-and-archive" };
  const labelMatch = normalized.match(/^(?:add-label|label):(.+)$/i);
  if (labelMatch && labelMatch[1]?.trim()) {
    return { kind: "label-add", label: labelMatch[1].trim() };
  }
  const removeLabelMatch = normalized.match(/^(?:remove-label|unlabel):(.+)$/i);
  if (removeLabelMatch && removeLabelMatch[1]?.trim()) {
    return { kind: "label-remove", label: removeLabelMatch[1].trim() };
  }
  const moveMatch = normalized.match(/^move:(.+)$/i);
  if (moveMatch && moveMatch[1]?.trim()) {
    return { kind: "move", label: moveMatch[1].trim() };
  }
  return null;
}

export async function runMailActions(
  actions: SupportedRuleAction[],
  ids: string[],
): Promise<MutationResponse> {
  if (actions.length === 2 && actions[0]?.kind === "read" && actions[1]?.kind === "archive") {
    return readAndArchiveMessages(ids);
  }
  const last = await actions.reduce<Promise<MutationResponse | null>>(
    (previous, action) => previous.then(() => runMailAction(action, ids)),
    Promise.resolve(null),
  );
  if (!last) throw new Error("No actions to apply");
  return last;
}

function runMailAction(action: SupportedRuleAction, ids: string[]): Promise<MutationResponse> {
  switch (action.kind) {
    case "archive":
      return archiveMessages(ids);
    case "trash":
      return trashMessages(ids);
    case "spam":
      return spamMessages(ids);
    case "star":
      return starMessages(ids, true);
    case "read":
      return markReadMessages(ids, true);
    case "unread":
      return markReadMessages(ids, false);
    case "read-and-archive":
      return readAndArchiveMessages(ids);
    case "label-add":
      return modifyLabels(ids, [action.label], []);
    case "label-remove":
      return modifyLabels(ids, [], [action.label]);
    case "move":
      return moveMessagesToLabel(ids, action.label);
  }
}
