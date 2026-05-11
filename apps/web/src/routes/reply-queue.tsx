import { createFileRoute } from "@tanstack/react-router";

import { ReplyQueueRoute } from "@/features/reply-queue/ReplyQueueRoute";

export const Route = createFileRoute("/reply-queue")({
  component: ReplyQueueRoute,
});
