/* @vitest-environment node */

import { describe, expect, test } from "vitest";

type SupportedRuleAction =
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

function mailActions(value: string): SupportedRuleAction[] | null {
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

describe("rules mailAction parser", () => {
  test("parses ordered action chains", () => {
    expect(mailActions("mark-read,archive")).toEqual([{ kind: "read" }, { kind: "archive" }]);
    expect(mailActions("label:Follow Up; archive")).toEqual([
      { kind: "label-add", label: "Follow Up" },
      { kind: "archive" },
    ]);
  });
});
