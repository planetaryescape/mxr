import { useEffect, useId } from "react";
import { create } from "zustand";

interface LitReasonState {
  owner: string | null;
  reason: string | null;
  note: string | null;
  accountId: string | null;
  set: (owner: string, reason: string, note: string | null, accountId: string | null) => void;
  clear: (owner: string) => void;
}

export const useLitReasonStore = create<LitReasonState>((set) => ({
  owner: null,
  reason: null,
  note: null,
  accountId: null,
  set: (owner, reason, note, accountId) => set({ owner, reason, note, accountId }),
  clear: (owner) =>
    set((state) =>
      state.owner === owner ? { owner: null, reason: null, note: null, accountId: null } : state,
    ),
}));

export function useLitReason(
  reason: string | null | undefined,
  note?: string | null,
  accountId?: string | null,
): void {
  const owner = useId();
  useEffect(() => {
    const { set, clear } = useLitReasonStore.getState();
    if (reason) set(owner, reason, note ?? null, accountId ?? null);
    else clear(owner);
  }, [accountId, note, owner, reason]);
  useEffect(() => () => useLitReasonStore.getState().clear(owner), [owner]);
}
