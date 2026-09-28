/*
 * Where the web's sounds come from. Mail verbs and sweeps call `playSound`
 * where they announce success; sends finish after their undo window, often
 * once the composer has closed, so they are heard through the send events.
 */

import { onSendEvent } from "@/features/compose/session/sendEvents";
import type { MailAction } from "@/features/mail-actions/pendingMailOps";
import { soundFor } from "@/features/mail-actions/verbFeedback";

import { getActiveQueryClient } from "@/lib/queryClient";

import { chimeSettingsQuery } from "./api";
import { installSoundPlayer, playSound } from "./player";

/**
 * The chime setting is shared with the TUI and CLI (`mxr chimes disable`),
 * so reread it whenever the page comes back into view. The player reads the
 * cache when it plays; nothing renders from it.
 */
function followSettingChanges(): () => void {
  const refresh = () => {
    if (document.hidden) return;
    void getActiveQueryClient()?.prefetchQuery({ ...chimeSettingsQuery, staleTime: 0 });
  };
  window.addEventListener("focus", refresh);
  document.addEventListener("visibilitychange", refresh);
  return () => {
    window.removeEventListener("focus", refresh);
    document.removeEventListener("visibilitychange", refresh);
  };
}

/** Mount once in the shell. Returns a cleanup. */
export function installSoundFeedback(): () => void {
  const stopPlayer = installSoundPlayer();
  const stopFollowing = followSettingChanges();
  const stopSends = onSendEvent((event) => {
    const sound = soundFor("send");
    if (event.kind === "sent" && sound) playSound(sound);
  });
  return () => {
    stopPlayer();
    stopFollowing();
    stopSends();
  };
}

/** One sound per mail action, however many messages it moved. */
export function playMailActionSound(action: MailAction): void {
  const event = soundFor(action);
  if (event) playSound(event);
}
