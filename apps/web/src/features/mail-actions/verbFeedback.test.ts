import { describe, expect, it } from "vitest";

import { getRegistry } from "@/lib/actions";

import { verb } from "./actionPastTense";
import { VERB_FEEDBACK } from "./verbFeedback";

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
  "modes.move",
  "modes.move-sender",
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

});
