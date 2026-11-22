import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/messages/$threadId")({
  component: () => null,
});
