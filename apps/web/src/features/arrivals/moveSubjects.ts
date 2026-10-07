/*
 * `X` and `K` on a mode's rows: the email each row stands for. Now's
 * Updates card is a digest of many emails, so it has none to move; a
 * sender's bundle in a place stands for its newest email.
 */

import type { PlaceBundle, PlaceMessage } from "@/features/places/api";
import type { NowItem } from "@/features/now/nowItems";
import { bundleSender } from "@/features/places/placeCopy";

import { answerPendingSenderAsk, openMovePicker, type MoveSubject } from "./moves";

export function nowMoveSubject(item: NowItem): MoveSubject | null {
  switch (item.kind) {
    case "person": {
      const row = item.person.row;
      return {
        messageId: row.message_id,
        label: row.subject || "(no subject)",
        senderLabel: row.counterparty_name || row.counterparty_email,
      };
    }
    case "todo": {
      const todo = item.todo.todo;
      if (!todo.source_message_id) return null;
      return {
        messageId: todo.source_message_id,
        label: todo.title,
        senderLabel: todo.counterparty ?? undefined,
      };
    }
    case "reading":
      return {
        messageId: item.pick.message_id,
        label: item.pick.subject || "(no subject)",
        senderLabel: item.pick.sender_name || item.pick.sender_email,
      };
    case "updates":
      return null;
  }
}

export function placeMoveSubject(bundle: PlaceBundle, message?: PlaceMessage): MoveSubject | null {
  const email = message ?? bundle.messages[0];
  if (!email) return null;
  return {
    messageId: email.message_id,
    label: email.subject || "(no subject)",
    senderLabel: bundleSender(bundle),
  };
}

/** The controller commands a view registers for `X` and `K`. */
export function moveCommands(subject: () => MoveSubject | null) {
  const open = (sender: boolean) => {
    const found = subject();
    if (found) openMovePicker(found, sender);
  };
  return {
    moveToMode: () => open(false),
    moveSenderToMode: () => {
      if (!answerPendingSenderAsk()) open(true);
    },
  };
}
