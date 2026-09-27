import { apiFetch } from "@/api/client";

export interface OwedReplyRow {
  thread_id: string;
  latest_inbound_msg_id: string;
  from_email: string;
  from_name?: string | null;
  subject: string;
  latest_inbound_at: string;
  waiting_days: number;
  expected_days?: number | null;
  overdue_score: number;
}

export function fetchOwedReplies(
  account?: string | null,
  limit = 200,
): Promise<{ rows: OwedReplyRow[] }> {
  const query = new URLSearchParams({ limit: String(limit) });
  if (account) query.set("account", account);
  return apiFetch<{ rows: OwedReplyRow[] }>(`/api/v1/mail/owed?${query.toString()}`);
}
