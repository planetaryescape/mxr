import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

/** One reply you owe: the daemon's `OwedReplyRowData`. */
export type OwedReplyRow = components["schemas"]["OwedReplyRowData"];

/** Conversations the page loads at most. */
export const OWED_PAGE_LIMIT = 200;

/** The daemon's people split: `write_with` is who these rows are with. */
export type OwedPeople = components["schemas"]["PeopleSplitData"];

export function fetchOwedReplies(
  account?: string | null,
): Promise<{ rows: OwedReplyRow[]; people?: OwedPeople | null }> {
  const query = new URLSearchParams({ limit: String(OWED_PAGE_LIMIT) });
  if (account) query.set("account", account);
  return apiFetch<{ rows: OwedReplyRow[]; people?: OwedPeople | null }>(
    `/api/v1/mail/owed?${query.toString()}`,
  );
}
