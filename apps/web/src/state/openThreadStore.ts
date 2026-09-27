/*
 * The conversation currently open in the reader, if any. Set by the reader
 * while it is mounted; action predicates and palette actions read it
 * instead of guessing from the URL.
 */

import { create } from "zustand";

interface OpenThreadState {
  threadId: string | null;
  setThreadId: (threadId: string | null) => void;
}

export const useOpenThread = create<OpenThreadState>((set) => ({
  threadId: null,
  setThreadId: (threadId) => set({ threadId }),
}));
