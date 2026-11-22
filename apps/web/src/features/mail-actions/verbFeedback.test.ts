import { describe, expect, it } from "vitest";

import { getRegistry } from "@/lib/actions";

import { VERB_FEEDBACK } from "./verbFeedback";

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

  it("covers every state-changing action in the registry", () => {
    expect(STATE_CHANGING.filter((id) => !triggers.has(id))).toEqual([]);
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
});
