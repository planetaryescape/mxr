import { createFileRoute } from "@tanstack/react-router";

import { ThreadRoute } from "@/features/thread/ThreadRoute";

export const Route = createFileRoute("/m/$mailbox/$threadId")({
  component: ThreadRoute,
});
