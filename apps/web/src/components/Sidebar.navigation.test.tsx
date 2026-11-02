/* @vitest-environment jsdom */

import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
  useSearch,
} from "@tanstack/react-router";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { parseArchiveSearch } from "@/features/archive/archiveSearch";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { Sidebar } from "./Sidebar";

vi.mock("@/components/AccountSwitcher", () => ({ AccountSwitcher: () => null }));
vi.mock("@/features/mailbox/useMailboxQuery", () => ({ useShellQuery: () => ({}) }));
vi.mock("@/features/desk/api", async (original) => ({
  ...(await original<typeof import("@/features/desk/api")>()),
  useDeskQuery: () => ({}),
}));
vi.mock("@/features/modes/rail", async (original) => ({
  ...(await original<typeof import("@/features/modes/rail")>()),
  useRailQuery: () => ({}),
}));

afterEach(() => act(() => useUiPrefs.setState({ accountScope: null })));

function ArchiveLookup() {
  const search = useSearch({ strict: false });
  return (
    <p>Lookup: {search.q ?? "empty"}; record: {search.record ?? "none"}</p>
  );
}

test("sidebar Archive resumes the lookup after another mode, including on reload", async () => {
  Element.prototype.scrollIntoView = vi.fn<() => void>();
  const root = createRootRoute({
    component: () => (
      <TooltipProvider>
        <Sidebar collapsed={false} />
        <Outlet />
      </TooltipProvider>
    ),
  });
  const archive = createRoute({
    getParentRoute: () => root,
    path: "/archive",
    validateSearch: parseArchiveSearch,
    component: ArchiveLookup,
  });
  const reading = createRoute({
    getParentRoute: () => root,
    path: "/reading",
    component: () => <p>Reading edition</p>,
  });
  const routeTree = root.addChildren([archive, reading]);
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: ["/archive?q=lisbon&record=tap&checked=true"] }),
  });
  const view = render(<RouterProvider router={router} />);
  await screen.findByText("Lookup: lisbon; record: tap");
  fireEvent.click(screen.getByRole("link", { name: /^Reading/ }));
  await screen.findByText("Reading edition");
  fireEvent.click(screen.getByRole("link", { name: /^Archive/ }));
  await screen.findByText("Lookup: lisbon; record: tap");
  expect(router.state.location.search).toMatchObject({ q: "lisbon", record: "tap", checked: true });
  const restoredUrl = router.state.location.href;
  view.unmount();
  const restored = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [restoredUrl] }),
  });
  render(<RouterProvider router={restored} />);
  await waitFor(() => expect(screen.getByText("Lookup: lisbon; record: tap")).toBeVisible());
});
