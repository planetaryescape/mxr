/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from "@tanstack/react-router";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { Topbar } from "./Topbar";
import { shellKey } from "@/features/mailbox/api";
import { useModals } from "@/state/modalStore";
import { usePageTitleStore } from "@/state/pageTitleStore";

describe("Topbar route changes", () => {
  test("exposes the existing keyboard help dialog from phone chrome", async () => {
    const originalMatchMedia = window.matchMedia;
    try {
      window.matchMedia = (() => ({
        matches: true,
        media: "(max-width: 639px)",
        addEventListener: vi.fn<MediaQueryList["addEventListener"]>(),
        removeEventListener: vi.fn<MediaQueryList["removeEventListener"]>(),
      })) as unknown as typeof window.matchMedia;

      const rootRoute = createRootRoute({ component: () => <Topbar /> });
      const route = createRoute({ getParentRoute: () => rootRoute, path: "/now" });
      const router = createRouter({
        routeTree: rootRoute.addChildren([route]),
        history: createMemoryHistory({ initialEntries: ["/now"] }),
      });
      const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      queryClient.setQueryData(shellKey, {});
      queryClient.setQueryData(["admin-status-is-demo"], { is_demo: false });
      await router.load();

      render(
        <QueryClientProvider client={queryClient}>
          <RouterProvider router={router} />
        </QueryClientProvider>,
      );

      const help = screen.getByRole("button", { name: "Keyboard help" });
      expect(help).toBeVisible();
      expect(help).toHaveClass("h-8", "w-8");
      fireEvent.click(help);
      expect(useModals.getState().helpOpen).toBe(true);
    } finally {
      useModals.setState({ helpOpen: false });
      window.matchMedia = originalMatchMedia;
    }
  });

  test("switches between matched and unmatched routes without changing hook order", async () => {
    const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    const rootRoute = createRootRoute({
      notFoundComponent: () => null,
      component: () => (
        <>
          <Topbar />
          <Outlet />
        </>
      ),
    });
    const nowRoute = createRoute({
      getParentRoute: () => rootRoute,
      path: "/now",
    });
    const history = createMemoryHistory({ initialEntries: ["/now"] });
    const router = createRouter({ routeTree: rootRoute.addChildren([nowRoute]), history });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(shellKey, {});
    queryClient.setQueryData(["admin-status-is-demo"], { is_demo: false });
    await router.load();

    render(
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    const breadcrumb = screen.getByRole("navigation", { name: "Breadcrumb" });
    await waitFor(() => expect(breadcrumb).toHaveTextContent("Now"));

    await act(async () => history.push("/not-a-route"));
    await waitFor(() => expect(breadcrumb).toHaveTextContent("Not found"));

    await act(async () => history.push("/now"));
    await waitFor(() => expect(breadcrumb).toHaveTextContent("Now"));
    scrollTo.mockRestore();
  });

  test("a lone crumb is dropped while the page publishes the same title", async () => {
    const rootRoute = createRootRoute({ component: () => <Topbar /> });
    const route = createRoute({ getParentRoute: () => rootRoute, path: "/now" });
    const router = createRouter({
      routeTree: rootRoute.addChildren([route]),
      history: createMemoryHistory({ initialEntries: ["/now"] }),
    });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(shellKey, {});
    queryClient.setQueryData(["admin-status-is-demo"], { is_demo: false });
    await router.load();

    render(
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    const breadcrumb = screen.getByRole("navigation", { name: "Breadcrumb" });
    await waitFor(() => expect(breadcrumb).toHaveTextContent("Now"));

    act(() => usePageTitleStore.getState().setTitle("Now"));
    expect(breadcrumb).not.toHaveTextContent("Now");

    act(() => usePageTitleStore.getState().setTitle("Messages"));
    expect(breadcrumb).toHaveTextContent("Now");
    act(() => usePageTitleStore.getState().setTitle(null));
  });
});
