/*
 * Where the web's sounds come from. Mail verbs and sweeps call `playSound`
 * where they announce success; sends finish after their undo window, often
 * once the composer has closed, so they are heard through the send events.
 */

import { onSendEvent } from "@/features/compose/session/sendEvents";
import type { MailAction } from "@/features/mail-actions/pendingMailOps";

import { installSoundPlayer, playSound, type SoundEvent } from "./player";

/** Mount once in the shell. Returns a cleanup. */
export function installSoundFeedback(): () => void {
  const stopPlayer = installSoundPlayer();
  const stopSends = onSendEvent((event) => {
    if (event.kind === "sent") playSound("sent");
  });
  return () => {
    stopPlayer();
    stopSends();
  };
}

const MAIL_ACTION_SOUNDS: Partial<Record<MailAction, SoundEvent>> = {
  archive: "archived",
  "read-and-archive": "archived",
  snooze: "snoozed",
};

/** One sound per mail action, however many messages it moved. */
export function playMailActionSound(action: MailAction): void {
  const event = MAIL_ACTION_SOUNDS[action];
  if (event) playSound(event);
}
