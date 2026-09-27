import type { QueryClient } from "@tanstack/react-query";

import { getActiveQueryClient } from "@/lib/queryClient";

/** Query families that show mail and must refresh after any mutation. */
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
  "snoozed",
  "saved-search-counts",
]);

/**
 * Refetch every mail view, active or cached. Resolves when the active ones
 * have the server's answer.
 */
export async function invalidateMailQueries(qc: QueryClient | null = getActiveQueryClient()) {
  if (!qc) return;
  await qc.invalidateQueries({
    predicate: (query) => MAIL_QUERY_ROOTS.has(String(query.queryKey[0])),
  });
}
