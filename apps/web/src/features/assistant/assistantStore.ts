/*
 * The assistant sidebar: whether it is open (closed until someone opens
 * it) and which conversation it shows. Both survive a reload; the
 * conversations themselves live in the daemon.
 */

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

interface AssistantUiState {
  open: boolean;
  conversationId: string | null;
  /** Bumped to ask the composer to take focus (on open, on a new chat). */
  focusRequest: number;
  setOpen: (open: boolean) => void;
  toggle: () => void;
  showConversation: (conversationId: string | null) => void;
  newConversation: () => void;
}

export const useAssistantUi = create<AssistantUiState>()(
  persist(
    (set) => ({
      open: false,
      conversationId: null,
      focusRequest: 0,
      setOpen: (open) =>
        set((s) => ({
          open,
          focusRequest: open ? s.focusRequest + 1 : s.focusRequest,
        })),
      toggle: () =>
        set((s) => ({
          open: !s.open,
          focusRequest: s.open ? s.focusRequest : s.focusRequest + 1,
        })),
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
