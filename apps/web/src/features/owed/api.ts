import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

/** One reply you owe: the daemon's `OwedReplyRowData`. */
export type OwedReplyRow = components["schemas"]["OwedReplyRowData"];

export function fetchOwedReplies(account?: string | null): Promise<{ rows: OwedReplyRow[] }> {
  const query = new URLSearchParams({ limit: "200" });
  if (account) query.set("account", account);
  return apiFetch<{ rows: OwedReplyRow[] }>(`/api/v1/mail/owed?${query.toString()}`);
}
