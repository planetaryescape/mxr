/*
 * Where the selection goes when the open row leaves its list: the next row
 * that is still there, else the previous one, else nothing (the pane
 * closes). One rule for every list with a detail pane beside it, as in
 * Gmail, Superhuman and the TUI.
 */

/**
 * The row to open after `removed` left the list. `before` is the list as it
 * was, in display order; `stillThere` says whether a row is in the list
 * now. Returns null when `removed` wasn't in `before`, or nothing near it
 * is left.
 */
export function nextAfterRemoval(
  before: readonly string[],
  removed: string,
  stillThere: (id: string) => boolean,
): string | null {
  const at = before.indexOf(removed);
  if (at < 0) return null;
  const kept = (id: string) => id !== removed && stillThere(id);
  const after = before.slice(at + 1).find(kept);
  if (after !== undefined) return after;
  return before.slice(0, at).findLast(kept) ?? null;
}
