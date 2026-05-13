import { useNavigate } from "@tanstack/react-router";
import { useCallback } from "react";

import { useDaemonEvents } from "@/hooks/useDaemonEvents";
import { useUiPrefs } from "@/state/uiPrefsStore";

export function useNewMessageNotifier(): void {
  const navigate = useNavigate();
  const enabled = useUiPrefs((state) => state.notificationsEnabled);
  const notifyAll = useUiPrefs((state) => state.notifyAllNewMail);
  const vipAllowlist = useUiPrefs((state) => state.vipAllowlist);

  useDaemonEvents(
    useCallback(
      (event) => {
        if (
          !enabled ||
          typeof Notification === "undefined" ||
          Notification.permission !== "granted"
        )
          return;
        if (event.type !== "NewMessages") return;
        const rawEvent = event as { envelopes?: unknown };
        const envelopes = Array.isArray(rawEvent.envelopes)
          ? (rawEvent.envelopes as Array<Record<string, unknown>>)
          : [];
        const first = envelopes[0];
        const sender = senderEmail(first);
        if (!notifyAll && !matchesVip(sender, vipAllowlist)) return;
        const notification = new Notification(first?.subject ? String(first.subject) : "New mail", {
          body: sender || "mxr received new mail",
        });
        notification.addEventListener(
          "click",
          () => {
            window.focus();
            const threadId = typeof first?.thread_id === "string" ? first.thread_id : undefined;
            if (threadId)
              void navigate({
                to: "/m/$mailbox/$threadId",
                params: { mailbox: "inbox", threadId },
              });
            else void navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
          },
          { once: true },
        );
      },
      [enabled, navigate, notifyAll, vipAllowlist],
    ),
  );
}

function senderEmail(envelope: Record<string, unknown> | undefined): string {
  const from = envelope?.from;
  if (typeof from === "object" && from !== null && "email" in from)
    return String((from as { email?: unknown }).email ?? "");
  return "";
}

function matchesVip(email: string, allowlist: string[]): boolean {
  const lower = email.toLowerCase();
  return allowlist.some((pattern) => {
    const value = pattern.toLowerCase().trim();
    if (!value) return false;
    if (value.startsWith("@")) return lower.endsWith(value);
    return lower === value;
  });
}
