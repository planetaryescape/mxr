import { createFileRoute } from "@tanstack/react-router";

import { ThreadPane } from "@/features/thread/ThreadPane";
import { optionalString } from "@/lib/searchParams";

export interface OpenThreadParams {
  /** A message to open and bring into view, e.g. a draft's cited source. */
  message?: string;
  /** The row's account: a thread id can be held by two accounts. */
  account?: string;
}

export const Route = createFileRoute("/m/$mailbox/$threadId")({
  validateSearch: (search: Record<string, unknown>): OpenThreadParams => ({
    message: optionalString(search.message),
    account: optionalString(search.account),
  }),
  component: OpenThread,
});

function OpenThread() {
  const { threadId } = Route.useParams();
  const { message } = Route.useSearch();
  return <ThreadPane threadId={threadId} focusMessageId={message} />;
}
