/*
 * Keys 1 to 4 on a lit new sender (Now and Messages): keep them in Messages,
 * Updates or Reading, or block them. Each is a scoped command that the page
 * registers only while a new sender's row is lit, so the same digits stay
 * the numbered tabs everywhere else (see `newSenderCommands`).
 */

import type { ActionBehavior, ActionScope } from "@/lib/actions/types";

import { SENDER_CHOICES } from "./NewSenderChips";

export function newSenderActions(
  scope: Extract<ActionScope, "now" | "messages">,
): ActionBehavior[] {
  return SENDER_CHOICES.map(({ key }) => ({
    id: `${scope}.sender-${key}`,
    command: `senderChoice${key}`,
  }));
}
