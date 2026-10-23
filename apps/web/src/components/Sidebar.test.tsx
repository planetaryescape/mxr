/* @vitest-environment jsdom */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { runCommand } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { Sidebar } from "./Sidebar";

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: ReactNode }) => <a href="/">{children}</a>,
  useNavigate: () => vi.fn<() => void>(),
  useRouterState: ({ select }: { select: (state: unknown) => unknown }) =>
    select({ location: { pathname: "/m/inbox", search: {} } }),
}));
vi.mock("@/components/AccountSwitcher", () => ({ AccountSwitcher: () => null }));
vi.mock("@/features/mailbox/useMailboxQuery", () => ({
  useShellQuery: () => ({ data: undefined }),
}));
vi.mock("@/features/desk/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/desk/api")>()),
  useDeskQuery: () => ({ data: undefined }),
}));
vi.mock("@/features/modes/rail", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/rail")>()),
  useRailQuery: () => ({ data: undefined }),
}));

afterEach(() => {
  act(() => useMailboxPane.getState().setActivePane("mailbox"));
});

function Harness({ open }: { open: boolean }) {
  return (
    <TooltipProvider>
      <Sidebar collapsed={false} />
      {/* Opened by a shortcut: no trigger to hand focus back to. */}
      <Dialog open={open}>
        <DialogContent aria-describedby={undefined}>
          <DialogTitle>Keys</DialogTitle>
        </DialogContent>
      </Dialog>
    </TooltipProvider>
  );
}

describe("Sidebar", () => {
  test("takes focus back when an overlay closes while it drives the keys", async () => {
    act(() => useMailboxPane.getState().setActivePane("sidebar"));
    const { rerender } = render(<Harness open />);
    await waitFor(() =>
      expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement),
    );

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    rerender(<Harness open={false} />);

    await waitFor(() => expect(document.querySelector('[data-active-pane="true"]')).toHaveFocus());
  });

  test("Enter on a section's header folds it, and { and } jump between headers", () => {
    const folded = useUiPrefs.getState().collapsedSections;
    // jsdom has no layout; the cursor-follow scroll has nothing to do.
    Element.prototype.scrollIntoView = vi.fn<() => void>();
    act(() => {
      useMailboxPane.getState().setActivePane("sidebar");
      useMailboxPane.getState().setSidebarIndex(0);
    });
    render(<Harness open={false} />);

    act(() => void runCommand("sidebar", "nextSection"));
    const at = useMailboxPane.getState().sidebarIndex;
    expect(document.querySelector(`[data-nav-index="${at}"]`)).toHaveTextContent("Mail");
    const wasFolded = useUiPrefs.getState().collapsedSections.includes("mail");

    act(() => void runCommand("sidebar", "open"));
    expect(useUiPrefs.getState().collapsedSections.includes("mail")).toBe(!wasFolded);
    expect(useMailboxPane.getState().sidebarIndex).toBe(at);

    act(() => void runCommand("sidebar", "prevSection"));
    expect(useMailboxPane.getState().sidebarIndex).toBe(0);
    act(() => useUiPrefs.setState({ collapsedSections: folded }));
  });
});
