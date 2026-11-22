import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type OpenVia = "pointer" | "key";

interface AssistantUiState {
  open: boolean;
  via: OpenVia | null;
  conversationId: string | null;
  focusRequest: number;
  setOpen: (open: boolean, via?: OpenVia) => void;
  toggle: (via: OpenVia) => void;
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
