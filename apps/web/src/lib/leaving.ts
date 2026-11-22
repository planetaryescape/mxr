export type LeaveVia = "key" | "pointer";

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

export function resetLeaving(): void {
  lastKeyLeave = Number.NEGATIVE_INFINITY;
}
