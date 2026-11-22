import { create } from "zustand";

import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import type { ComposeKind, InviteReplyAction } from "./api";
import type { ComposeIntent } from "./useComposeSession";

export type ComposeSurface = "inline" | "overlay" | "fullscreen";

export interface ComposeCommands {
  intentKey: string;
  send: () => void;
  sendAndRemind: () => void;
  draftForMe: () => void;
}

export interface ComposeUiState {
  intent: ComposeIntent | null;
  surface: ComposeSurface;
  commands: ComposeCommands | null;
  openCompose: (intent: ComposeIntent, surface?: ComposeSurface) => void;
  setSurface: (surface: ComposeSurface) => void;
  closeCompose: () => void;
  setCommands: (commands: ComposeCommands | null) => void;
}

export const useComposeUi = create<ComposeUiState>((set, get) => ({
  intent: null,
  surface: "overlay",
  commands: null,
  openCompose: (
    intent,
    surface = intent.kind === "reply" || intent.kind === "reply_all" ? "inline" : "overlay",
  ) => {
    if (get().intent?.key !== intent.key && refuseWhileDaemonDown("open the composer")) return;
    set({ intent, surface });
  },
  setSurface: (surface) => set({ surface }),
  closeCompose: () => set({ intent: null }),
  setCommands: (commands) => set({ commands }),
}));

export function newMessageIntent(): ComposeIntent {
  return { key: `compose:new:${crypto.randomUUID()}`, title: "New message", kind: "new" };
}

export function prefilledMessageIntent(to?: string, subject?: string): ComposeIntent {
  const prefillTo = to?.trim() || undefined;
  const prefillSubject = subject?.trim() || undefined;
  return {
    key: `compose:new:${crypto.randomUUID()}`,
    title: "New message",
    kind: "new",
    prefillTo,
    prefillSubject,
  };
}

export function draftIntent(draftId: string): ComposeIntent {
  return { key: `draft:${draftId}`, title: "Saved draft", kind: "new", draftId };
}

const inviteReplyTitles: Record<InviteReplyAction, string> = {
  accept: "Accept with comment",
  tentative: "Tentative with comment",
  decline: "Decline with comment",
};

export function inviteReplyIntent(messageId: string, action: InviteReplyAction): ComposeIntent {
  return {
    key: `compose:invite_reply:${action}:${messageId}`,
    title: inviteReplyTitles[action],
    kind: "invite_reply",
    messageId,
    inviteAction: action,
  };
}

export function intentFromComposeLocation(
  pathname: string,
  search: Record<string, unknown>,
): ComposeIntent {
  const draftMatch = pathname.match(/^\/compose\/([^/]+)$/);
  const draftId = draftMatch?.[1] ? decodeURIComponent(draftMatch[1]) : undefined;
  if (draftId && draftId !== "new") return draftIntent(draftId);
  const reply = typeof search.reply === "string" ? search.reply : undefined;
  if (reply) {
    const mode =
      search.mode === "forward" || search.mode === "all" || search.mode === "single"
        ? search.mode
        : "single";
    return replyIntent(reply, mode);
  }
  return prefilledMessageIntent(
    typeof search.to === "string" ? search.to : undefined,
    typeof search.subject === "string" ? search.subject : undefined,
  );
}

export function replyWithBodyIntent(messageId: string, body: string): ComposeIntent {
  return {
    ...replyIntent(messageId, "single"),
    key: `compose:reply:${messageId}:draft-${Date.now()}`,
    prefillBody: body,
  };
}

export function replyIntent(messageId: string, mode: "single" | "all" | "forward"): ComposeIntent {
  const kind: ComposeKind = mode === "forward" ? "forward" : mode === "all" ? "reply_all" : "reply";
  const title =
    kind === "forward" ? "Forward message" : kind === "reply_all" ? "Reply all" : "Reply";
  return {
    key: kind === "forward" ? `compose:forward:${messageId}` : `compose:reply:${messageId}`,
    title,
    kind,
    messageId,
    quiet: kind !== "forward",
  };
}
