import { createFileRoute, useParams } from "@tanstack/react-router";

import { MessagesRoute, type MessagesSearch } from "@/features/messages/MessagesRoute";
import { optionalEnum, optionalFlag, optionalString } from "@/lib/searchParams";

const TURNS = ["all", "mine", "theirs"] as const;

export const Route = createFileRoute("/messages")({
  validateSearch: (search: Record<string, unknown>): MessagesSearch => ({
    person: optionalString(search.person),
    topic: optionalString(search.topic),
    turn: optionalEnum(search.turn, TURNS) ?? (search.lane === "waiting" ? "theirs" : undefined),
    unread: optionalFlag(search.unread),
    pace: optionalFlag(search.pace),
  }),
  component: Messages,
});

function Messages() {
  const search = Route.useSearch();
  const { threadId } = useParams({ strict: false });
  return <MessagesRoute search={search} threadId={threadId} />;
}
