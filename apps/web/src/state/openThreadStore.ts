import { create } from "zustand";

interface OpenThreadState {
  threadId: string | null;
  setThreadId: (threadId: string | null) => void;
}

export const useOpenThread = create<OpenThreadState>((set) => ({
  threadId: null,
  setThreadId: (threadId) => set({ threadId }),
}));
