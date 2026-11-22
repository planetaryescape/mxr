/* @vitest-environment jsdom */
import { beforeEach, expect, test, vi } from "vitest";
import { operationAfterReview } from "./sendOutcomes";

beforeEach(() => {
  const items = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => items.set(key, value),
    clear: () => items.clear(),
  });
});

test("an explicit unchanged review permits a fresh attempt only after proven non-acceptance", () => {
  const prior = {
    identity: { operation_id: "draft:1", expected_revision: 1 },
    state: "not_accepted" as const,
    local_message_id: null,
  };
  expect(operationAfterReview("draft:1", prior)).toMatch(/^draft:1:retry:/);
  expect(() => operationAfterReview("draft:1", { ...prior, state: "unknown" })).toThrow(
    "uncertain",
  );
  expect(() => operationAfterReview("draft:1", { ...prior, state: "accepted" })).toThrow(
    "already accepted",
  );
  expect(() => operationAfterReview("draft:1", { ...prior, state: "waiting" })).toThrow(
    "retry automatically",
  );
});
