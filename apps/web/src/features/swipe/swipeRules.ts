export type SwipeAction = "archive" | "done" | "trash" | "snooze" | "sweep" | "later" | "letgo";

export interface SwipeMap {
  right?: SwipeAction;
  rightLong?: SwipeAction;
  left?: SwipeAction;
}

export const SWIPE = {
  axisLock: 8,
  reveal: 40,
  commit: 90,
  long: 190,
  flickVelocity: 0.5,
  flickDistance: 30,
} as const;

export type Axis = "pending" | "x" | "y";

export function lockAxis(dx: number, dy: number): Axis {
  if (Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE.axisLock) return "pending";
  return Math.abs(dx) > Math.abs(dy) ? "x" : "y";
}

export interface Pending {
  action: SwipeAction;
  side: "left" | "right";
  armed: boolean;
}

export function pendingAt(dx: number, velocity: number, map: SwipeMap): Pending | null {
  const commit = releaseAt(dx, velocity, map);
  if (commit) return { action: commit, side: dx > 0 ? "right" : "left", armed: true };
  if (dx >= SWIPE.reveal && map.right) return { action: map.right, side: "right", armed: false };
  if (dx <= -SWIPE.reveal && map.left) return { action: map.left, side: "left", armed: false };
  return null;
}

export function releaseAt(dx: number, velocity: number, map: SwipeMap): SwipeAction | null {
  const flicking = Math.abs(velocity) >= SWIPE.flickVelocity;
  if (dx >= SWIPE.long && map.rightLong && !flicking) return map.rightLong;
  const flickRight = velocity >= SWIPE.flickVelocity && dx >= SWIPE.flickDistance;
  if (map.right && (dx >= SWIPE.commit || flickRight)) return map.right;
  const flickLeft = velocity <= -SWIPE.flickVelocity && dx <= -SWIPE.flickDistance;
  if (map.left && (dx <= -SWIPE.commit || flickLeft)) return map.left;
  return null;
}

export function followDistance(dx: number, map: SwipeMap): number {
  const limit = dx > 0 ? (map.rightLong ? Infinity : SWIPE.commit) : SWIPE.commit;
  const magnitude = Math.abs(dx);
  if (magnitude <= limit) return dx;
  const extra = magnitude - limit;
  return Math.sign(dx) * (limit + extra * 0.25);
}

export const SWIPE_WORDS: Record<SwipeAction, string> = {
  archive: "Archive",
  done: "Done",
  trash: "Trash",
  snooze: "Snooze",
  sweep: "Sweep sender",
  later: "Later",
  letgo: "Let go",
};
