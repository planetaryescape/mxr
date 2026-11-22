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
  } catch {}
}

export function panesWidth(group: HTMLElement | null): number {
  if (!group) return 0;
  let total = 0;
  for (const child of group.children) {
    if (child instanceof HTMLElement && child.hasAttribute("data-panel"))
      total += child.offsetWidth;
  }
  return total;
}
