import { onSendEvent } from "@/features/compose/session/sendEvents";
import type { MailAction } from "@/features/mail-actions/pendingMailOps";
import { soundFor } from "@/features/mail-actions/verbFeedback";

import { getActiveQueryClient } from "@/lib/queryClient";

import { chimeSettingsQuery } from "./api";
import { installSoundPlayer, playSound } from "./player";

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

export function playMailActionSound(action: MailAction): void {
  const event = soundFor(action);
  if (event) playSound(event);
}
