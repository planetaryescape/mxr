import { createFileRoute, redirect } from "@tanstack/react-router";

/** A typed or bookmarked /settings opens the section list, which picks the first tab on wide screens. */
export const Route = createFileRoute("/settings/")({
  beforeLoad: () => {
    throw redirect({ to: "/settings/$section", params: { section: "all" }, replace: true });
  },
});
