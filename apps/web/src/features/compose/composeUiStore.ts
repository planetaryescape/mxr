/*
 * Where the composer is hosted: inline at the bottom of the open thread,
 * as an overlay sheet, or fullscreen. ComposeHost (mounted once in
 * AppShell) is the only compose surface: it owns the session for whichever
 * intent is open, so switching surfaces never unmounts the editor or loses
 * the draft buffer. The /compose/new and /compose/$draftId routes are deep
 * links that open this host (see intentFromComposeLocation).
 */

import { create } from "zustand";

import type { ComposeKind, InviteReplyAction } from "./api";
import type { ComposeIntent } from "./useComposeSession";

export type ComposeSurface = "inline" | "overlay" | "fullscreen";

export interface ComposeUiState {
  intent: ComposeIntent | null;
  surface: ComposeSurface;
  openCompose: (intent: ComposeIntent, surface?: ComposeSurface) => void;
  setSurface: (surface: ComposeSurface) => void;
  closeCompose: () => void;
}

export const useComposeUi = create<ComposeUiState>((set) => ({
  intent: null,
  surface: "overlay",
  openCompose: (intent, surface = "overlay") => set({ intent, surface }),
  setSurface: (surface) => set({ surface }),
  closeCompose: () => set({ intent: null }),
}));

/** Intent for a brand-new message (the global `c` shortcut). */
export function newMessageIntent(): ComposeIntent {
  return { key: "compose:new:new", title: "New message", kind: "new" };
}

/** New message with optional To / Subject prefill (compose launcher, deep
 * links). Without prefill it is the same intent, and draft, as `c`. */
export function prefilledMessageIntent(to?: string, subject?: string): ComposeIntent {
  const prefillTo = to?.trim() || undefined;
  const prefillSubject = subject?.trim() || undefined;
  const prefillKey = [prefillTo ?? "", prefillSubject ?? ""].join("|");
  if (prefillKey === "|") return newMessageIntent();
  return {
    key: `compose:new:${prefillKey}`,
    title: "New message",
    kind: "new",
    prefillTo,
    prefillSubject,
  };
}

/** Intent for editing a stored mxr draft. */
export function draftIntent(draftId: string): ComposeIntent {
  return { key: `draft:${draftId}`, title: "Saved draft", kind: "new", draftId };
}

const inviteReplyTitles: Record<InviteReplyAction, string> = {
  accept: "Accept with comment",
  tentative: "Tentative with comment",
  decline: "Decline with comment",
};

/** Intent for answering a calendar invite with a written comment. The
 * bridge prefills the reply and attaches the iCal REPLY on send. */
export function inviteReplyIntent(messageId: string, action: InviteReplyAction): ComposeIntent {
  return {
    key: `compose:invite_reply:${action}:${messageId}`,
    title: inviteReplyTitles[action],
    kind: "invite_reply",
    messageId,
    inviteAction: action,
  };
}

/**
 * Intent for a `/compose/new?...` or `/compose/$draftId` deep link, so those
 * URLs open the same host as every other compose entry point.
 */
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

/** Intent for reply/reply-all/forward on a thread's primary message. */
export function replyIntent(
  messageId: string,
  mode: "single" | "all" | "forward",
): ComposeIntent {
  const kind: ComposeKind = mode === "forward" ? "forward" : mode === "all" ? "reply_all" : "reply";
  const title =
    kind === "forward" ? "Forward message" : kind === "reply_all" ? "Reply all" : "Reply";
  return { key: `compose:${kind}:${messageId}`, title, kind, messageId };
}
