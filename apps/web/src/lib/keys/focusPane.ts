/**
 * Put DOM focus back on the pane the keyboard was driving. Dialogs opened
 * from a shortcut have no trigger to return to, and focus on <body> makes
 * screen readers lose their place. The mailbox panes mark themselves with
 * `data-active-pane`; Now, the modes, Screener and Focus have one main view,
 * marked `data-mode-pane`, which stands in when no pane is marked.
 */
export function focusActivePane(): void {
  const pane =
    document.querySelector<HTMLElement>('[data-active-pane="true"]') ??
    document.querySelector<HTMLElement>("[data-mode-pane]") ??
    document.querySelector<HTMLElement>("[data-page-pane]");
  pane?.focus({ preventScroll: true });
}

/**
 * After `closing` (a promise that settles once the view that held focus is
 * gone, such as a navigation), give the active pane focus if it fell to
 * <body>. Closing a mode's reader removed the focused reader and left focus
 * nowhere, where Inbox hands it back to the list.
 */
export async function refocusPaneAfter(closing: Promise<unknown>): Promise<void> {
  await closing.catch(() => undefined);
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  const active = document.activeElement;
  if (!active || active === document.body || !active.isConnected) focusActivePane();
}
