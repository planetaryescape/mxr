import { createFileRoute } from "@tanstack/react-router";

import { ThreadPane } from "@/features/thread/ThreadPane";

/** A record's email, opened beside the ledger as evidence (`o`). */
export const Route = createFileRoute("/archive/$threadId")({
  component: OpenThread,
});

function OpenThread() {
  const { threadId } = Route.useParams();
  return <ThreadPane threadId={threadId} />;
}
