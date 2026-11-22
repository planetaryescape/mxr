export function focusActivePane(): void {
  const pane =
    document.querySelector<HTMLElement>('[data-active-pane="true"]') ??
    document.querySelector<HTMLElement>("[data-mode-pane]") ??
    document.querySelector<HTMLElement>("[data-page-pane]");
  pane?.focus({ preventScroll: true });
}

export async function refocusPaneAfter(closing: Promise<unknown>): Promise<void> {
  await closing.catch(() => undefined);
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  const active = document.activeElement;
  if (!active || active === document.body || !active.isConnected) focusActivePane();
}
