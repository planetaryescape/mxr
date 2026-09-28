/*
 * Where the web's sounds come from. Mail verbs and sweeps call `playSound`
 * where they announce success; sends finish after their undo window, often
 * once the composer has closed, so they are heard through the send events.
 */

import { onSendEvent } from "@/features/compose/session/sendEvents";
import type { MailAction } from "@/features/mail-actions/pendingMailOps";
import { soundFor } from "@/features/mail-actions/verbFeedback";

import { installSoundPlayer, playSound } from "./player";

/** Mount once in the shell. Returns a cleanup. */
export function installSoundFeedback(): () => void {
  const stopPlayer = installSoundPlayer();
  const stopSends = onSendEvent((event) => {
    const sound = soundFor("send");
    if (event.kind === "sent" && sound) playSound(sound);
  });
  return () => {
    stopPlayer();
    stopSends();
  };
}

/** One sound per mail action, however many messages it moved. */
export function playMailActionSound(action: MailAction): void {
  const event = soundFor(action);
  if (event) playSound(event);
}
