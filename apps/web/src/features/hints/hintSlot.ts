/*
 * The one place a hint can show. Rules (blueprint 22, "The app teaches
 * itself in place"):
 *
 * - At most one hint at a time, never stacked.
 * - Never on arrival: a hint waits until the user has pressed a key or
 *   clicked on this page, unless its element is the only thing there.
 * - No chains: once a hint leaves, another shows only when its own element
 *   becomes needed afterwards, never because the slot freed up.
 */

import { create } from "zustand";

interface HintSlotState {
  /** The hint on screen, by id. */
  active: string | null;
  /**
   * The page (first path segment) the user last pressed a key or clicked
   * on. The key that navigates here was pressed on the page before, so
   * arriving never counts.
   */
  interactedOn: string | null;
  /**
   * Counts hints leaving. A hint whose element became needed before the
   * last one left waits for its next need, so dismissing one never
   * reveals another.
   */
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

/** "/todo/abc" and "/todo" are the same page for hints. */
export function pageOf(pathname: string): string {
  return pathname.split("/")[1] ?? "";
}

let listening = false;

/** Note the page of every key press and click, once per app. */
export function listenForInteraction(): void {
  if (listening || typeof window === "undefined") return;
  listening = true;
  const note = () => {
    const page = pageOf(window.location.pathname);
    if (useHintSlot.getState().interactedOn !== page) useHintSlot.setState({ interactedOn: page });
  };
  // Capture, so a handler that stops the event still counts as a use.
  window.addEventListener("keydown", note, true);
  window.addEventListener("pointerdown", note, true);
}
