/*
 * Saved pane widths, in pixels, one localStorage key per split. Pixels,
 * not the panel library's percentages: a percentage is only meaningful
 * against the window it was taken in. localStorage can throw (private
 * windows, blocked site data) and hold anything; both read as "no saved
 * width", and the pane opens at its default.
 */

export function readPaneWidth(key: string): number | null {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(key) ?? "null");
    return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

export function writePaneWidth(key: string, value: number | null): void {
  try {
    if (value === null) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, JSON.stringify(Math.round(value)));
  } catch {
    // Quota or a blocked store: the width lasts this session only.
  }
}

/**
 * The width the library divides between a group's panes (theirs, not the
 * handles'). The panes' own widths lag a layout change by a render; their
 * sum doesn't, so `share% * panesWidth` is a pane's new width.
 */
export function panesWidth(group: HTMLElement | null): number {
  if (!group) return 0;
  let total = 0;
  for (const child of group.children) {
    if (child instanceof HTMLElement && child.hasAttribute("data-panel"))
      total += child.offsetWidth;
  }
  return total;
}
