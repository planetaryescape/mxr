/*
 * Where the composer is hosted: inline at the bottom of the open thread,
 * as an overlay sheet, or fullscreen. ComposeHost (mounted once in
 * AppShell) is the only compose surface: it owns the session for whichever
 * intent is open, so switching surfaces never unmounts the editor or loses
 * the draft buffer. The /compose/new and /compose/$draftId routes are deep
 * links that open this host (see intentFromComposeLocation).
 */

import { create } from "zustand";

import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import type { ComposeKind, InviteReplyAction } from "./api";
import type { ComposeIntent } from "./useComposeSession";

export type ComposeSurface = "inline" | "overlay" | "fullscreen";

/**
 * What a view hosting the inline composer (focus mode) can ask of the open
 * session without owning it: the keys work from outside the editor too.
 */
export interface ComposeCommands {
  /** The intent these commands act on, so a stale page can't send another. */
  intentKey: string;
  send: () => void;
  /** Open "send and remind me if nobody replies" with its time field. */
  sendAndRemind: () => void;
  /** Open Draft for me and ask for a draft in the user's voice. */
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
  openCompose: (intent, surface = "overlay") => {
    // A draft is a file the daemon writes; without it the composer could
    // only spin. One already open stays usable.
    if (get().intent?.key !== intent.key && refuseWhileDaemonDown("open the composer")) return;
    set({ intent, surface });
  },
  setSurface: (surface) => set({ surface }),
  closeCompose: () => set({ intent: null }),
  setCommands: (commands) => set({ commands }),
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

/**
 * A reply that starts from an AI draft. Its own session key, so it never
 * resumes an earlier reply session that lacks the draft.
 */
export function replyWithBodyIntent(messageId: string, body: string): ComposeIntent {
  return {
    ...replyIntent(messageId, "single"),
    key: `compose:reply:${messageId}:draft-${Date.now()}`,
    prefillBody: body,
  };
}

/** Intent for reply/reply-all/forward on a thread's primary message. */
export function replyIntent(messageId: string, mode: "single" | "all" | "forward"): ComposeIntent {
  const kind: ComposeKind = mode === "forward" ? "forward" : mode === "all" ? "reply_all" : "reply";
  const title =
    kind === "forward" ? "Forward message" : kind === "reply_all" ? "Reply all" : "Reply";
  return { key: `compose:${kind}:${messageId}`, title, kind, messageId };
}
