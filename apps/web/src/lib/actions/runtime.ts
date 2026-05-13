/*
 * Runtime services that action runners need but cannot call as React hooks.
 * AppShell (PR #3) calls `setRuntimeNavigate(nav)` once on mount; PR #2 ships
 * with a stub navigate that warns. Until then runners are dormant — they
 * exist so the catalog can be exhaustive but no consumer invokes them yet.
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
