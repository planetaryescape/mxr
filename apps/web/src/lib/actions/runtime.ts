import { useMailboxPane } from "@/state/mailboxPaneStore";

interface Navigator {
  navigate: (to: string) => void;
}

let nav: Navigator | null = null;

export function setRuntimeNavigate(next: Navigator): void {
  nav = next;
}

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
