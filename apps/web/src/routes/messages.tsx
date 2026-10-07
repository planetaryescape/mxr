import { createFileRoute, useParams } from "@tanstack/react-router";

import { MessagesRoute, type MessagesSearch } from "@/features/messages/MessagesRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

const TURNS = ["mine", "theirs"] as const;

/** Messages: people you talk with, one row each (blueprint 22, phase 3). */
export const Route = createFileRoute("/messages")({
  validateSearch: (search: Record<string, unknown>): MessagesSearch => ({
    person: optionalString(search.person),
    topic: optionalString(search.topic),
    // `lane=waiting` was Waiting on in the early version: it is turn=theirs.
    turn:
      optionalEnum(search.turn, TURNS) ?? (search.lane === "waiting" ? "theirs" : undefined),
  }),
  component: Messages,
});

function Messages() {
  const search = Route.useSearch();
  // /messages/$threadId opens the row holding that conversation.
  const { threadId } = useParams({ strict: false });
  return <MessagesRoute search={search} threadId={threadId} />;
}
