import { describe, expect, it } from "vitest";

import { getRegistry } from "@/lib/actions";

import { verb } from "./actionPastTense";
import type { MailAction } from "./pendingMailOps";
import { toneFor, VERB_FEEDBACK } from "./verbFeedback";

/** Registry actions that change mail or a place: each must be a verb's trigger. */
const STATE_CHANGING = [
  "mail.archive",
  "mail.read-archive",
  "mail.trash",
  "mail.spam",
  "mail.star",
  "mail.mark-read",
  "mail.mark-unread",
  "mail.label",
  "mail.move",
  "mail.route",
  "mail.snooze",
  "mail.unsubscribe",
  "mail.reply-later",
  "reader.archive-next",
  "reader.archive-prev",
  "reader.move-sender",
  "list.row-action",
  "place.pin",
  "place.sweep-bundle",
  "place.sweep-all",
  "place.move-sender",
  "place.unsubscribe",
  "focus.send",
  "focus.remind",
  "focus.snooze",
  "todo.done",
  "todo.dismiss",
  "todo.schedule",
  "todo.edit",
  "todo.catchup-keep",
  "todo.catchup-let-go",
  "todo.catchup-let-go-all",
  "mail.make-todo",
  "mail.pass-to-mode",
  "archive.dismiss",
  "archive.edit",
  "archive.check",
];

describe("verb feedback table", () => {
  const triggers = new Set(Object.values(VERB_FEEDBACK).flatMap((entry) => entry.actions));

  it("names only actions that exist", () => {
    const missing = [...triggers].filter((id) => getRegistry().get(id) === undefined);
    expect(missing).toEqual([]);
  });

  it("covers every state-changing action in the registry", () => {
    expect(STATE_CHANGING.filter((id) => !triggers.has(id))).toEqual([]);
    // Anything else that runs a mail command is either listed above or
    // doesn't change mail (export, briefing, links…).
    const readOnly = new Set([
      "mail.export",
      "mail.briefing",
      "mail.whois",
      "mail.sender-profile",
      "mail.links",
      "mail.attachments",
    ]);
    const unlisted = getRegistry()
      .all()
      .filter((action) => action.group === "Mail" && action.command !== undefined)
      .filter((action) => !action.paletteOnly)
      .map((action) => action.id)
      .filter((id) => !triggers.has(id) && !readOnly.has(id));
    expect(unlisted).toEqual([]);
  });

  it("gives every verb its four parts", () => {
    const incomplete = Object.entries(VERB_FEEDBACK)
      .filter(([, entry]) => !entry.optimistic || !entry.pastTense || !entry.undo)
      .map(([name]) => name);
    expect(incomplete).toEqual([]);
  });

  it("is where the toasts get their words", () => {
    const actions: MailAction[] = ["archive", "trash", "spam", "snooze", "read-and-archive"];
    expect(actions.map((action) => verb(action))).toEqual(
      actions.map((action) => VERB_FEEDBACK[action].pastTense),
    );
    expect(verb("move", { label: "Work" })).toBe("Moved to Work");
  });

  it("marks undo-less verbs irreversible, with a reason, and they confirm first", () => {
    const unguarded = Object.entries(VERB_FEEDBACK)
      .filter(
        ([, entry]) =>
          entry.undo === "none" && (!entry.irreversible || !entry.reason || !entry.confirm),
      )
      .map(([name]) => name);
    expect(unguarded).toEqual([]);
  });

  it("words label changes from the table, naming the label", () => {
    expect(verb("labels", { add: ["Hiring"] })).toBe("Labelled Hiring");
    expect(verb("labels", { remove: ["Hiring"] })).toBe("Removed Hiring from");
    expect(verb("labels", { add: ["Hiring"], remove: ["Travel"] })).toBe(
      VERB_FEEDBACK.labels.pastTense,
    );
  });
});

describe("toast tones", () => {
  it("colours done green and neutral changes with an undo in the accent", () => {
    const done = ["desk-done", "mode-done", "todo-done", "send"] as const;
    expect(done.map(toneFor)).toEqual(done.map(() => "success"));
    const neutral = ["archive", "move", "reply-later-at", "snooze", "sweep"] as const;
    expect(neutral.map(toneFor)).toEqual(neutral.map(() => "info"));
  });

  it("never reports a verb's result as a warning or an error", () => {
    // Those tones are for failures, which have their own toasts.
    const off = Object.entries(VERB_FEEDBACK).filter(
      ([, entry]) => entry.tone !== "success" && entry.tone !== "info",
    );
    expect(off.map(([name]) => name)).toEqual([]);
  });
});
