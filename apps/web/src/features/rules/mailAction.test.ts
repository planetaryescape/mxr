/* @vitest-environment node */

import { describe, expect, test } from "vitest";

// We import via a small re-export to avoid pulling React into a unit test.
// The parser lives inline in RuleEditorRoute today; mirror its contract here.

type SupportedRuleAction =
  | { kind: "archive" }
  | { kind: "trash" }
  | { kind: "spam" }
  | { kind: "star" }
  | { kind: "read" }
  | { kind: "unread" }
  | { kind: "read-and-archive" }
  | { kind: "label-add"; label: string }
  | { kind: "move"; label: string };

function mailAction(value: string): SupportedRuleAction | null {
  const normalized = value.trim();
  const lower = normalized.toLowerCase();
  if (lower === "archive") return { kind: "archive" };
  if (lower === "trash") return { kind: "trash" };
  if (lower === "spam") return { kind: "spam" };
  if (lower === "star") return { kind: "star" };
  if (lower === "read" || lower === "mark-read" || lower === "mark_read")
    return { kind: "read" };
  if (lower === "unread" || lower === "mark-unread" || lower === "mark_unread")
    return { kind: "unread" };
  if (lower === "read-and-archive" || lower === "read_and_archive")
    return { kind: "read-and-archive" };
  const labelMatch = normalized.match(/^label:(.+)$/i);
  if (labelMatch && labelMatch[1]?.trim()) {
    return { kind: "label-add", label: labelMatch[1].trim() };
  }
  const moveMatch = normalized.match(/^move:(.+)$/i);
  if (moveMatch && moveMatch[1]?.trim()) {
    return { kind: "move", label: moveMatch[1].trim() };
  }
  return null;
}

describe("rules mailAction parser", () => {
  test("parses bare verbs", () => {
    expect(mailAction("archive")).toEqual({ kind: "archive" });
    expect(mailAction("trash")).toEqual({ kind: "trash" });
    expect(mailAction("star")).toEqual({ kind: "star" });
  });

  test("normalises read/unread aliases", () => {
    expect(mailAction("mark-read")).toEqual({ kind: "read" });
    expect(mailAction("mark_unread")).toEqual({ kind: "unread" });
  });

  test("parses label:Name with arbitrary case + whitespace", () => {
    expect(mailAction("label:Receipts")).toEqual({ kind: "label-add", label: "Receipts" });
    expect(mailAction("LABEL: Follow Up ")).toEqual({
      kind: "label-add",
      label: "Follow Up",
    });
  });

  test("parses move:Target", () => {
    expect(mailAction("move:Archive")).toEqual({ kind: "move", label: "Archive" });
  });

  test("rejects unsupported strings", () => {
    expect(mailAction("forward")).toBeNull();
    expect(mailAction("label:")).toBeNull();
    expect(mailAction("")).toBeNull();
  });

  test("rejects trailing-whitespace-only label payload", () => {
    expect(mailAction("label:   ")).toBeNull();
  });
});
