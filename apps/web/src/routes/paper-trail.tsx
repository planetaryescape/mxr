import { createFileRoute, redirect } from "@tanstack/react-router";

/** Paper trail became Updates; old links and bookmarks still land there. */
export const Route = createFileRoute("/paper-trail")({
  beforeLoad: ({ location }) => {
    // An open conversation redirects in its own route, keeping the thread.
    if (location.pathname.replace(/\/$/, "") !== "/paper-trail") return;
    throw redirect({ to: "/updates", replace: true });
  },
});
