/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { AppShell } from "./AppShell";

const state = vi.hoisted(() => ({
  accounts: [{ account_id: "a-1" }],
  pathname: "/onboarding",
  outletMounts: 0,
}));

vi.mock("@tanstack/react-router", async () => {
  const React = await import("react");
  return {
    Outlet: function StatefulOutlet() {
      const [step, setStep] = React.useState(1);
      React.useEffect(() => {
        state.outletMounts += 1;
      }, []);
      return (
        <div data-testid="route-content">
          <span>{step === 4 ? "First sync" : `Setup step ${step}`}</span>
          <button type="button" onClick={() => setStep(4)}>
            Complete account setup
          </button>
        </div>
      );
    },
    useNavigate: () => vi.fn<() => void>(),
    useRouterState: ({
      select,
    }: {
      select: (state: { location: { pathname: string } }) => string;
    }) => select({ location: { pathname: state.pathname } }),
  };
});

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: async () => ({ accounts: state.accounts ?? [] }),
}));

vi.mock("@/components/Sidebar", () => ({ Sidebar: () => <nav aria-label="Mailboxes" /> }));
vi.mock("@/components/Topbar", () => ({ Topbar: () => <div data-testid="topbar" /> }));
vi.mock("@/components/StatusBar", () => ({ StatusBar: () => <div data-testid="statusbar" /> }));
vi.mock("@/components/MobileTabs", () => ({ MobileTabs: () => null }));
vi.mock("@/components/OfflineBanner", () => ({ OfflineBanner: () => null }));
// Panels measure a real layout, which jsdom does not have.
vi.mock("@/components/ui/resizable", () => ({
  ResizablePanelGroup: ({ children, className }: { children: ReactNode; className?: string }) => (
    <div className={className}>{children}</div>
  ),
  ResizablePanel: ({ children, className }: { children: ReactNode; className?: string }) => (
    <div className={className}>{children}</div>
  ),
  ResizableHandle: () => null,
}));
vi.mock("@/hooks/useResizableSidebar", () => ({
  SIDEBAR_ID: "sidebar",
  SIDEBAR_SIZE: { collapsedSize: 4, minSize: 10, maxSize: 30 },
  useResizableSidebar: () => ({
    frameRef: { current: null },
    groupProps: {},
    panelRef: { current: null },
    handleProps: {},
    defaultSize: 20,
    onResize: () => {},
  }),
}));
vi.mock("@/hooks/useKeyDispatcher", () => ({ useKeyDispatcher: () => {} }));
vi.mock("@/features/notifications/useNewMessageNotifier", () => ({
  useNewMessageNotifier: () => {},
}));
vi.mock("@/features/promises/promiseOffers", () => ({
  usePromiseOffers: (select: (s: { offers: unknown[] }) => unknown) => select({ offers: [] }),
}));
vi.mock("@/features/sound/feedback", () => ({ installSoundFeedback: () => () => {} }));
vi.mock("@/lib/actions/keyHints", () => ({ installKeyHints: () => () => {} }));

function renderShell(ui: ReactNode) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return {
    ...render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>),
    queryClient,
  };
}

describe("AppShell with no account", () => {
  beforeEach(() => {
    state.accounts = [];
    state.pathname = "/onboarding";
    state.outletMounts = 0;
  });

  test("drops the rail, top bar and status bar, leaving only the route", async () => {
    renderShell(<AppShell />);

    // The shell renders before the accounts answer, so wait for the frame to
    // drop its chrome rather than checking once.
    await waitFor(() => expect(screen.queryByTestId("topbar")).toBeNull());
    expect(screen.getByTestId("route-content")).toBeVisible();
    expect(screen.queryByTestId("statusbar")).toBeNull();
    expect(screen.queryByRole("navigation", { name: "Mailboxes" })).toBeNull();
  });

  test("keeps onboarding mounted through the first account refetch", async () => {
    const view = renderShell(<AppShell />);
    await waitFor(() => expect(screen.getByText("Setup step 1")).toBeVisible());
    await waitFor(() => {
      expect(
        view.queryClient.getQueryData<{ accounts: unknown[] }>(["accounts"])?.accounts,
      ).toHaveLength(0);
      expect(screen.queryByTestId("topbar")).toBeNull();
    });
    const onboardingMounts = state.outletMounts;
    const outlet = screen.getByTestId("route-content");

    fireEvent.click(screen.getByRole("button", { name: "Complete account setup" }));
    expect(screen.getByText("First sync")).toBeVisible();

    await act(async () => {
      view.queryClient.setQueryData(["accounts"], { accounts: [{ account_id: "a-1" }] });
      view.rerender(
        <QueryClientProvider client={view.queryClient}>
          <AppShell />
        </QueryClientProvider>,
      );
    });

    expect(
      view.queryClient.getQueryData<{ accounts: unknown[] }>(["accounts"])?.accounts,
    ).toHaveLength(1);
    expect(screen.queryByTestId("topbar")).toBeNull();
    expect(screen.queryByTestId("statusbar")).toBeNull();
    expect(screen.getByTestId("route-content")).toBe(outlet);
    expect(screen.getByText("First sync")).toBeVisible();
    expect(state.outletMounts).toBe(onboardingMounts);

    state.pathname = "/now";
    view.rerender(
      <QueryClientProvider client={view.queryClient}>
        <AppShell />
      </QueryClientProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("topbar")).toBeVisible());
    expect(screen.getByTestId("statusbar")).toBeVisible();
    expect(screen.getByRole("navigation", { name: "Mailboxes" })).toBeVisible();
  });

  test("keeps the full shell once an account exists", async () => {
    state.accounts = [{ account_id: "a-1" }];
    state.pathname = "/now";
    renderShell(<AppShell />);

    await waitFor(() => expect(screen.getByTestId("topbar")).toBeVisible());
    expect(screen.getByTestId("statusbar")).toBeVisible();
    expect(screen.getByRole("navigation", { name: "Mailboxes" })).toBeVisible();
  });
});
