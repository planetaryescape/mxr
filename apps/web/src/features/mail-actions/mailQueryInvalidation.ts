import type { QueryClient } from "@tanstack/react-query";

import { getActiveQueryClient } from "@/lib/queryClient";

const MAIL_QUERY_ROOTS = new Set([
  "mailbox",
  "thread",
  "shell",
  "search",
  "search-groups",
  "search-palette",
  "reply-queue",
  "owed",
  "desk",
  "place",
  "message-kind",
  "snoozed",
  "saved-search-counts",
  "now",
  "messages-mode",
  "todos",
  "mode-membership",
  "reading",
  "arrivals",
  "arrival-modes",
  "updates",
]);

export async function invalidateMailQueries(qc: QueryClient | null = getActiveQueryClient()) {
  if (!qc) return;
  await qc.invalidateQueries({
    predicate: (query) => MAIL_QUERY_ROOTS.has(String(query.queryKey[0])),
  });
}
