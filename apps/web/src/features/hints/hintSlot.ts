import { create } from "zustand";

interface HintSlotState {
  active: string | null;
  interactedOn: string | null;
  epoch: number;
  claim: (id: string) => void;
  release: (id: string) => void;
}

export const useHintSlot = create<HintSlotState>((set) => ({
  active: null,
  interactedOn: null,
  epoch: 0,
  claim: (id) => set((s) => (s.active === null ? { active: id } : s)),
  release: (id) => set((s) => (s.active === id ? { active: null, epoch: s.epoch + 1 } : s)),
}));

export function pageOf(pathname: string): string {
  return pathname.split("/")[1] ?? "";
}

let listening = false;

export function listenForInteraction(): void {
  if (listening) return;
  listening = true;
  const note = () => {
    const page = pageOf(window.location.pathname);
    if (useHintSlot.getState().interactedOn !== page) useHintSlot.setState({ interactedOn: page });
  };
  window.addEventListener("keydown", note, true);
  window.addEventListener("pointerdown", note, true);
}
