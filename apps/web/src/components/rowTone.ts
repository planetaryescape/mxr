/** Missing attention state stays neutral; only explicit seen/read calms a row. */
export function rowPrimaryTone(unread?: boolean | null): string {
  return unread ? "font-semibold text-text-1" : unread === false ? "text-text-2" : "text-text-1";
}
