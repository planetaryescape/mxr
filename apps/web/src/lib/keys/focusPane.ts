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
