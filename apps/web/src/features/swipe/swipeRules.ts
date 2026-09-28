/*
 * Touch swipe on list rows, as rules. A short throw right archives (or
 * marks done), a long throw right trashes, a throw left snoozes. Colour and
 * words show the pending action from the reveal distance; nothing happens
 * until release, and only a release past the long threshold trashes (a
 * fast flick never does). The gesture locks to one axis after a few pixels
 * so vertical scrolling stays the browser's.
 */

export type SwipeAction = "archive" | "done" | "trash" | "snooze" | "sweep";

/** What a row does for each throw; a missing entry means that throw does nothing. */
export interface SwipeMap {
  right?: SwipeAction;
  /** Past the long threshold; destructive actions live only here. */
  rightLong?: SwipeAction;
  left?: SwipeAction;
}

export const SWIPE = {
  /** Movement before the gesture picks an axis. */
  axisLock: 8,
  /** The pending action shows from here. */
  reveal: 40,
  /** Released past this, the short action commits. */
  commit: 90,
  /** Released past this, the long (destructive) action commits. */
  long: 190,
  /** px/ms: a flick this fast commits a short action early. */
  flickVelocity: 0.5,
  /** A flick still has to travel this far. */
  flickDistance: 30,
} as const;

export type Axis = "pending" | "x" | "y";

/** Which way the gesture goes, once it has moved far enough to tell. */
export function lockAxis(dx: number, dy: number): Axis {
  if (Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE.axisLock) return "pending";
  return Math.abs(dx) > Math.abs(dy) ? "x" : "y";
}

export interface Pending {
  action: SwipeAction;
  side: "left" | "right";
  /** Releasing now would commit it. */
  armed: boolean;
}

/** What the row shows under it at `dx`, or null before the reveal distance. */
export function pendingAt(dx: number, map: SwipeMap): Pending | null {
  if (dx >= SWIPE.long && map.rightLong) {
    return { action: map.rightLong, side: "right", armed: true };
  }
  if (dx >= SWIPE.reveal && map.right) {
    return { action: map.right, side: "right", armed: dx >= SWIPE.commit };
  }
  if (dx <= -SWIPE.reveal && map.left) {
    return { action: map.left, side: "left", armed: -dx >= SWIPE.commit };
  }
  return null;
}

/**
 * The action a release commits. `velocity` is px/ms along x at release.
 * The long (destructive) action needs a slow, deliberate release past its
 * threshold; any release moving at flick speed resolves to the short
 * action or nothing, however far it went.
 */
export function releaseAt(dx: number, velocity: number, map: SwipeMap): SwipeAction | null {
  const flicking = Math.abs(velocity) >= SWIPE.flickVelocity;
  if (dx >= SWIPE.long && map.rightLong && !flicking) return map.rightLong;
  const flickRight = velocity >= SWIPE.flickVelocity && dx >= SWIPE.flickDistance;
  if (map.right && (dx >= SWIPE.commit || flickRight)) return map.right;
  const flickLeft = velocity <= -SWIPE.flickVelocity && dx <= -SWIPE.flickDistance;
  if (map.left && (dx <= -SWIPE.commit || flickLeft)) return map.left;
  return null;
}

/**
 * How far the row follows the finger: freely to the commit distance, then
 * with resistance where no further action waits, so it never runs away.
 */
export function followDistance(dx: number, map: SwipeMap): number {
  const limit = dx > 0 ? (map.rightLong ? Infinity : SWIPE.commit) : SWIPE.commit;
  const magnitude = Math.abs(dx);
  if (magnitude <= limit) return dx;
  const extra = magnitude - limit;
  return Math.sign(dx) * (limit + extra * 0.25);
}

export const SWIPE_WORDS: Record<SwipeAction, string> = {
  archive: "Archive",
  done: "Done waiting",
  trash: "Trash",
  snooze: "Snooze",
  sweep: "Sweep sender",
};
