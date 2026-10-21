/*
 * The assistant sidebar: whether it is open (closed until someone opens
 * it) and which conversation it shows. Both survive a reload; the
 * conversations themselves live in the daemon.
 */

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/** What opened the panel. Only a pointer slides it in; a key never animates (D134). */
export type OpenVia = "pointer" | "key";

interface AssistantUiState {
  open: boolean;
  /**
   * The pointer that is opening the panel, until its slide has played or the
   * panel closes; null otherwise. Not persisted, so a reload, a panel that
   * was already open and a remount never replay the slide.
   */
  via: OpenVia | null;
  conversationId: string | null;
  /** Bumped to ask the composer to take focus (on open, on a new chat). */
  focusRequest: number;
  setOpen: (open: boolean, via?: OpenVia) => void;
  toggle: (via: OpenVia) => void;
  /** The slide has played: later remounts (the drawer breakpoint) stay still. */
  settle: () => void;
  showConversation: (conversationId: string | null) => void;
  newConversation: () => void;
}

export const useAssistantUi = create<AssistantUiState>()(
  persist(
    (set) => ({
      open: false,
      via: null,
      conversationId: null,
      focusRequest: 0,
      setOpen: (open, via = "key") =>
        set((s) => ({
          open,
          via: open && !s.open && via === "pointer" ? "pointer" : open ? s.via : null,
          focusRequest: open ? s.focusRequest + 1 : s.focusRequest,
        })),
      toggle: (via) =>
        set((s) => ({
          open: !s.open,
          via: !s.open && via === "pointer" ? "pointer" : null,
          focusRequest: s.open ? s.focusRequest : s.focusRequest + 1,
        })),
      settle: () => set({ via: null }),
      showConversation: (conversationId) => set({ conversationId }),
      newConversation: () =>
        set((s) => ({
          conversationId: null,
          focusRequest: s.focusRequest + 1,
        })),
    }),
    {
      name: "mxr:assistant",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ open, conversationId }) => ({ open, conversationId }),
    },
  ),
);
