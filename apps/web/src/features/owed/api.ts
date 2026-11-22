import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type OwedReplyRow = components["schemas"]["OwedReplyRowData"];

export const OWED_PAGE_LIMIT = 200;

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
