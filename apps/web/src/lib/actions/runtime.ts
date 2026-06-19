/*
 * Runtime services that action runners need but cannot call as React hooks.
 * The keymap layer injects navigation once mounted; before that, calls warn so
 * early action invocations are visible.
 */

interface Navigator {
  navigate: (to: string) => void;
}

let nav: Navigator | null = null;

export function setRuntimeNavigate(next: Navigator): void {
  nav = next;
}

export function getRuntimeNavigate(): Navigator {
  if (nav) return nav;
  return {
    navigate: (to) => {
      // eslint-disable-next-line no-console
      console.warn(`[actions] navigate(${to}) before runtime ready`);
    },
  };
}
