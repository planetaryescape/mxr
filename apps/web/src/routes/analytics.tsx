import { Outlet, createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/analytics")({
  beforeLoad: ({ location }) => {
    if (location.pathname === "/analytics" || location.pathname === "/analytics/") {
      throw redirect({ to: "/analytics/$dashboard", params: { dashboard: "storage" } });
    }
  },
  component: Outlet,
});
