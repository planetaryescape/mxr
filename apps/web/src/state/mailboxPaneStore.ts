import { create } from "zustand";

export type MailPane = "sidebar" | "mailbox" | "reader";

interface MailboxPaneState {
  activePane: MailPane;
  sidebarIndex: number;
  setActivePane: (pane: MailPane) => void;
  setSidebarIndex: (index: number) => void;
}

export const useMailboxPane = create<MailboxPaneState>((set) => ({
  activePane: "mailbox",
  sidebarIndex: 0,
  setActivePane: (activePane) => set({ activePane }),
  setSidebarIndex: (sidebarIndex) => set({ sidebarIndex }),
}));
