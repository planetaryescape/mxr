import { create } from "zustand";

import type { ConnectionState } from "@/lib/ws";
import type { ProtocolMismatch } from "@/lib/protocolCompatibility";

export interface SyncProgress {
  account_id: string;
  current: number;
  total: number;
}

export interface ConnectionStoreState {
  state: ConnectionState;
  lastEventAt?: number;
  lastErrorAt?: number;
  errorMessage?: string;
  protocolMismatch?: ProtocolMismatch;
  protocolCheckedAt?: number;
  protocolCheckError?: string;
  syncProgress?: SyncProgress;
  daemonDown: boolean;
  daemonLastAnsweredAt?: number;
  semanticReindexProgress?: { current: number; total: number };
  setState: (next: Partial<Omit<ConnectionStoreState, keyof Setters>>) => void;
}

interface Setters {
  setState: ConnectionStoreState["setState"];
}

export const useConnectionStore = create<ConnectionStoreState>((set) => ({
  state: "offline",
  daemonDown: false,
  setState: (next) => set(next),
}));
