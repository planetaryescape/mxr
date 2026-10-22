import { describe, expect, it } from "vitest";

import type { Commitment } from "@/features/promises/api";

import { nextDuePromise } from "./useNextPromise";

const NOW = Date.parse("2026-10-22T09:00:00Z");

function commitment(id: string, by_when: string | null, date_unverified = false): Commitment {
  return {
    id,
    account_id: "a1",
    email: "nora@example.com",
    thread_id: "t1",
    direction: "yours",
    status: "open",
    who_owes: "me@example.com",
    what: `promise ${id}`,
    by_when,
    date_unverified,
    evidence_msg_id: "m1",
    extracted_at: "2026-10-01T09:00:00Z",
  };
}

describe("nextDuePromise", () => {
  it("skips a date a model wrote and points at the next one that counts", () => {
    const next = nextDuePromise(
      [
        commitment("model", "2026-10-23T09:00:00Z", true),
        commitment("words", "2026-10-25T09:00:00Z"),
        commitment("later", "2026-10-30T09:00:00Z"),
      ],
      NOW,
    );
    expect(next?.commitment.id).toBe("words");
  });

  it("returns nothing when every dated promise is unverified, past or theirs", () => {
    const theirs = {
      ...commitment("theirs", "2026-10-25T09:00:00Z"),
      direction: "theirs" as const,
    };
    expect(
      nextDuePromise(
        [
          commitment("model", "2026-10-25T09:00:00Z", true),
          commitment("past", "2026-10-01T09:00:00Z"),
          commitment("undated", null),
          theirs,
        ],
        NOW,
      ),
    ).toBeNull();
  });
});
