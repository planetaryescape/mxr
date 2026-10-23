/*
 * The list motions every mode inherits from the mail list (blueprint 35):
 * g g and G for the first and last row, Ctrl-d and Ctrl-u for half a page.
 * A mode's `move(delta)` already clamps to its rows, so the ends are the
 * largest steps. Mode rows differ in height and sit in different scroll
 * containers, so half a page is estimated from the window rather than
 * measured per list.
 */

const ROW_ESTIMATE_PX = 44;

export function halfPageRows(): number {
  return Math.max(1, Math.floor(window.innerHeight / 2 / ROW_ESTIMATE_PX));
}

export function listMotions(move: (delta: number) => void) {
  return {
    top: () => move(-Number.MAX_SAFE_INTEGER),
    bottom: () => move(Number.MAX_SAFE_INTEGER),
    pageDown: () => move(halfPageRows()),
    pageUp: () => move(-halfPageRows()),
  };
}
