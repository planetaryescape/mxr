/*
 * Where the composer is hosted: inline at the bottom of the open thread,
 * as an overlay sheet, or fullscreen. ComposeHost (mounted once in
 * AppShell) owns the session for whichever intent is open, so switching
 * surfaces never unmounts the editor or loses the draft buffer. The
 * /compose/$draftId route remains as a deep-link host with the same hook.
 */

import { create } from "zustand";

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

/** Intent for reply/reply-all/forward on a thread's primary message. */
export function replyIntent(
  messageId: string,
  mode: "single" | "all" | "forward",
): ComposeIntent {
  const kind = mode === "forward" ? "forward" : mode === "all" ? "reply_all" : "reply";
  const title =
    kind === "forward" ? "Forward message" : kind === "reply_all" ? "Reply all" : "Reply";
  return { key: `compose:${kind}:${messageId}`, title, kind, messageId };
}
