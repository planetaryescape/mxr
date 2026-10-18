import { describe, expect, test } from "vitest";

import { failureTitle, partialFailureTitle } from "./actionPastTense";
import type { MailAction, MailActionPayload } from "./pendingMailOps";
import { VERB_FEEDBACK } from "./verbFeedback";

const CASES: [MailAction, MailActionPayload | undefined, string, string][] = [
  [
    "archive",
    undefined,
    "Couldn't archive 3 messages",
    "Archived 2 of 3 messages; the rest failed",
  ],
  [
    "read-and-archive",
    undefined,
    "Couldn't mark 3 messages read and archive them",
    "Read and archived 2 of 3 messages; the rest failed",
  ],
  [
    "trash",
    undefined,
    "Couldn't move 3 messages to Trash",
    "Moved 2 of 3 messages to Trash; the rest failed",
  ],
  [
    "spam",
    undefined,
    "Couldn't mark 3 messages as spam",
    "Marked 2 of 3 messages as spam; the rest failed",
  ],
  ["star", undefined, "Couldn't star 3 messages", "Starred 2 of 3 messages; the rest failed"],
  ["unstar", undefined, "Couldn't unstar 3 messages", "Unstarred 2 of 3 messages; the rest failed"],
  [
    "read",
    undefined,
    "Couldn't mark 3 messages read",
    "Marked 2 of 3 messages read; the rest failed",
  ],
  [
    "unread",
    undefined,
    "Couldn't mark 3 messages unread",
    "Marked 2 of 3 messages unread; the rest failed",
  ],
  [
    "move",
    { label: "Hiring" },
    "Couldn't move 3 messages to Hiring",
    "Moved 2 of 3 messages to Hiring; the rest failed",
  ],
  ["move", undefined, "Couldn't move 3 messages", "Moved 2 of 3 messages; the rest failed"],
  [
    "route",
    { label: "Reply" },
    "Couldn't route 3 messages to Reply",
    "Routed 2 of 3 messages to Reply; the rest failed",
  ],
  [
    "label-add",
    { label: "Hiring" },
    "Couldn't add Hiring to 3 messages",
    "Added Hiring to 2 of 3 messages; the rest failed",
  ],
  [
    "label-remove",
    { label: "Hiring" },
    "Couldn't remove Hiring from 3 messages",
    "Removed Hiring from 2 of 3 messages; the rest failed",
  ],
  [
    "labels",
    { add: ["A"], remove: ["B"] },
    "Couldn't update labels on 3 messages",
    "Updated labels on 2 of 3 messages; the rest failed",
  ],
  [
    "labels",
    { add: ["A"] },
    "Couldn't add A to 3 messages",
    "Added A to 2 of 3 messages; the rest failed",
  ],
  [
    "snooze",
    { untilLabel: "Friday" },
    "Couldn't snooze 3 messages",
    "Snoozed 2 of 3 messages; the rest failed",
  ],
];

describe("failure toast titles", () => {
  test.each(CASES)("%s %j", (action, payload, full, partial) => {
    expect(failureTitle(action, payload, "3 messages")).toBe(full);
    expect(partialFailureTitle(action, payload, "2 of 3 messages")).toBe(partial);
  });

  test("a full failure never reads as a past-tense verb", () => {
    for (const [action, payload] of CASES) {
      expect(failureTitle(action, payload, "3 messages")).toMatch(/^Couldn't /);
      expect(failureTitle(action, payload, "3 messages")).not.toMatch(/failed/);
    }
  });

  test("every mail action has failure wording", () => {
    const actions = new Set(CASES.map(([action]) => action));
    for (const action of actions) expect(VERB_FEEDBACK[action].failure).toBeDefined();
  });
});
