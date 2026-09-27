import { createFileRoute } from "@tanstack/react-router";

import { ThreadPane } from "@/features/thread/ThreadPane";

export const Route = createFileRoute("/m/$mailbox/$threadId")({
  component: OpenThread,
});

function OpenThread() {
  const { threadId } = Route.useParams();
  return <ThreadPane threadId={threadId} />;
}
