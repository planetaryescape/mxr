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
