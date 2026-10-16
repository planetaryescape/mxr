import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

/** One reply you owe: the daemon's `OwedReplyRowData`. */
export type OwedReplyRow = components["schemas"]["OwedReplyRowData"];

/** The daemon's people split: `write_with` is who these rows are with. */
export type OwedPeople = components["schemas"]["PeopleSplitData"];

export function fetchOwedReplies(
  account?: string | null,
): Promise<{ rows: OwedReplyRow[]; people?: OwedPeople | null }> {
  const query = new URLSearchParams({ limit: "200" });
  if (account) query.set("account", account);
  return apiFetch<{ rows: OwedReplyRow[]; people?: OwedPeople | null }>(
    `/api/v1/mail/owed?${query.toString()}`,
  );
}
