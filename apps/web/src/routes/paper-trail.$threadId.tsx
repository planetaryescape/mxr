import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/paper-trail/$threadId")({
  beforeLoad: ({ params }) => {
    throw redirect({ to: "/updates/$threadId", params, replace: true });
  },
});
