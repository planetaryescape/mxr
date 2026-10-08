/*
 * `X` and `K` on a mode's rows: the email each row stands for. Now's
 * Updates card is a digest of many emails, so it has none to move; a
 * sender's bundle in a place stands for its newest email.
 */

import type { PlaceBundle, PlaceMessage } from "@/features/places/api";
import type { NowItem } from "@/features/now/nowItems";
import { bundleSender } from "@/features/places/placeCopy";
import type { EditionEntry } from "@/features/reading/readingView";
import type { UpdateLine } from "@/features/updates/api";

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

/** An Updates line stands for its newest email; `K` stays "tune this source". */
export function updateMoveSubject(line: UpdateLine): MoveSubject | null {
  const messageId = line.latest_message_id ?? line.fact_message_id;
  if (!messageId) return null;
  return { messageId, label: line.source_name, senderLabel: line.source_name };
}

/** A Reading item, or the issue a digest link came from. */
export function readingMoveSubject(entry: EditionEntry): MoveSubject | null {
  const item = entry.kind === "item" ? entry.item : entry.parent;
  return {
    messageId: item.message_id,
    label: entry.kind === "item" ? item.title : entry.link.title,
    senderLabel: item.source || item.sender_email,
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
