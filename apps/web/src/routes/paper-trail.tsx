import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/paper-trail")({
  beforeLoad: ({ location }) => {
    if (location.pathname.replace(/\/$/, "") !== "/paper-trail") return;
    throw redirect({ to: "/updates", replace: true });
  },
});
