import { create } from "zustand";

export type MailPane = "sidebar" | "mailbox" | "reader";

interface MailboxPaneState {
  activePane: MailPane;
  suppressNextReaderFocus: boolean;
  sidebarIndex: number;
  setActivePane: (pane: MailPane) => void;
  setSuppressNextReaderFocus: (suppress: boolean) => void;
  setSidebarIndex: (index: number) => void;
}

export const useMailboxPane = create<MailboxPaneState>((set) => ({
  activePane: "mailbox",
  suppressNextReaderFocus: false,
  sidebarIndex: 0,
  setActivePane: (activePane) => set({ activePane }),
  setSuppressNextReaderFocus: (suppressNextReaderFocus) => set({ suppressNextReaderFocus }),
  setSidebarIndex: (sidebarIndex) => set({ sidebarIndex }),
}));
