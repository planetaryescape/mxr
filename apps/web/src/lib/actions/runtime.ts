/*
 * Runtime services that action runners need but cannot call as React hooks.
 * The keymap layer injects navigation once mounted; before that, calls warn so
 * early action invocations are visible.
 */

import { useMailboxPane } from "@/state/mailboxPaneStore";

interface Navigator {
  navigate: (to: string) => void;
}

let nav: Navigator | null = null;

export function setRuntimeNavigate(next: Navigator): void {
  nav = next;
}

/**
 * Going somewhere by key (`g x`, a tab digit, the palette) hands the keys to
 * the page it opens, as a press in the page does. Left with the sidebar after
 * `h`, `j` would move the sidebar's hidden cursor instead of the new page's
 * list, and dialogs would return focus there.
 */
function releaseSidebar(): void {
  const pane = useMailboxPane.getState();
  if (pane.activePane === "sidebar") pane.setActivePane("mailbox");
}

export function getRuntimeNavigate(): Navigator {
  const current = nav;
  if (current) {
    return {
      navigate: (to) => {
        releaseSidebar();
        current.navigate(to);
      },
    };
  }
  return {
    navigate: (to) => {
      // eslint-disable-next-line no-console
      console.warn(`[actions] navigate(${to}) before runtime ready`);
    },
  };
}
