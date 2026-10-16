/*
 * Whether a row that is leaving folds away or just goes (docs/blueprint/27
 * "Rows", D101, D134). A pointer always folds: the fold ties the click to the
 * result. A key folds only for the first of a burst: one `e` shows where the
 * row went, but keyboard repeat, which is how people clear a list, stays
 * instant because every press inside the burst window drops its row at once.
 * Reduced motion never folds.
 */

export type LeaveVia = "key" | "pointer";

/** A key press this soon after the last one is repeat. */
export const KEY_BURST_MS = 600;

let lastKeyLeave = Number.NEGATIVE_INFINITY;

function motionReduced(): boolean {
  return document.documentElement.dataset.motion === "reduced";
}

export function foldsFor(via: LeaveVia, now: number = performance.now()): boolean {
  if (motionReduced()) return false;
  if (via === "pointer") return true;
  const repeat = now - lastKeyLeave < KEY_BURST_MS;
  lastKeyLeave = now;
  return !repeat;
}

/** Forget the burst, for tests. */
export function resetLeaving(): void {
  lastKeyLeave = Number.NEGATIVE_INFINITY;
}
