import { describe, expect, test } from "vitest";

import type { ModeDoneOutcome } from "@/features/modes/modeDone";

import type { NowUpdatesCard } from "./api";
import { letGoPreview } from "./letGoCopy";

const card = {
  message_count: 23,
  source_count: 9,
  top_sources: [],
  line: "23 updates from 9 sources.",
  early: true,
  since: "2026-10-03T07:00:00Z",
  thread_ids: ["a", "b", "c"],
} as NowUpdatesCard;

const outcome = (thread: string, extra: Partial<ModeDoneOutcome>): ModeDoneOutcome => ({
  thread_id: thread,
  mode: "updates",
  provider: "Gmail",
  copy: "Done. Archived in Gmail.",
  ...extra,
});

describe("letting go of the Updates card", () => {
  test("names what leaves the inbox and what To do keeps", () => {
    const preview = letGoPreview(card, [
      outcome("a", { archived: 2 }),
      outcome("b", { still_in: ["todo"], copy: "Done in Updates. Still in To do." }),
      outcome("c", { error: "not in Updates" }),
    ]);
    expect(preview.title).toBe("Let go of 23 updates from 9 sources?");
    expect(preview.note).toBe(
      "1 conversation leaves your inbox. 1 conversation also in To do stays there. Undo puts it back.",
    );
    expect(preview.threadIds).toEqual(["a", "b"]);
  });

  test("with the archive setting off, nothing leaves the inbox", () => {
    const preview = letGoPreview(card, [outcome("a", {}), outcome("b", {})]);
    expect(preview.note).toBe("Nothing leaves your inbox. Undo puts it back.");
  });
});
