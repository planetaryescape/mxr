import { createFileRoute } from "@tanstack/react-router";

import { ThreadPane } from "@/features/thread/ThreadPane";

export const Route = createFileRoute("/search/$threadId")({
  component: OpenSearchResult,
});

function OpenSearchResult() {
  const { threadId } = Route.useParams();
  return <ThreadPane threadId={threadId} />;
}
