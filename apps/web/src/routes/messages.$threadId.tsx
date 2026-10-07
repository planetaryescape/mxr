import { createFileRoute } from "@tanstack/react-router";

/**
 * A conversation in Messages. The parent page reads the thread id and opens
 * the person (or group) holding it, with that topic selected.
 */
export const Route = createFileRoute("/messages/$threadId")({
  component: () => null,
});
