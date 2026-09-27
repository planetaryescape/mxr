/**
 * Put DOM focus back on the pane the keyboard was driving. Dialogs opened
 * from a shortcut have no trigger to return to, and focus on <body> makes
 * screen readers lose their place.
 */
export function focusActivePane(): void {
  const pane = document.querySelector<HTMLElement>('[data-active-pane="true"]');
  pane?.focus({ preventScroll: true });
}
