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
import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { Topbar } from "./Topbar";
import { shellKey } from "@/features/mailbox/api";

describe("Topbar route changes", () => {
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
});
