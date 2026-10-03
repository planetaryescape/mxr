import { describe, expect, test } from "vitest";

import { doneToast, hiddenKey, type ModeDoneOutcome } from "./modeDone";

const outcome = (thread: string, copy: string): ModeDoneOutcome => ({
  thread_id: thread,
  mode: "messages",
  provider: "Gmail",
  copy,
});

describe("done here's toast", () => {
  test("one thread says where it went, in the daemon's words", () => {
    expect(doneToast([outcome("a", "Done in Messages. Still in To do (due Mon).")])).toBe(
      "Done in Messages. Still in To do (due Mon).",
    );
  });

  test("several that went the same way say how many and where", () => {
    expect(
      doneToast([
        outcome("a", "Done. Archived in Gmail."),
        outcome("b", "Done. Archived in Gmail."),
      ]),
    ).toBe("2 conversations: Done. Archived in Gmail.");
  });

  test("several that went different ways say how many", () => {
    expect(
      doneToast([
        outcome("a", "Done. Archived in Gmail."),
        outcome("b", "Done in Messages. Still in To do."),
      ]),
    ).toBe("Done with 2 conversations.");
    expect(doneToast([])).toBe("");
  });

  test("hides a row per mode, so done in one mode leaves the others", () => {
    expect(hiddenKey("messages", "t1")).not.toBe(hiddenKey("todo", "t1"));
  });
});
