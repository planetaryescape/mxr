import { apiFetch } from "@/api/client";

export interface ScreenerEntry {
  sender_email: string;
  display_name?: string | null;
  message_count: number;
  latest_subject: string;
  latest_at: string;
}

export type ScreenerDisposition = "allow" | "deny" | "feed" | "paper_trail" | "unknown";

export function fetchScreenerQueue(accountId: string) {
  return apiFetch<{ entries: ScreenerEntry[] }>(
    `/api/v1/mail/screener/queue?account_id=${accountId}&limit=100`,
  );
}

export function setScreenerDecision(input: {
  accountId: string;
  senderEmail: string;
  disposition: ScreenerDisposition;
}) {
  return apiFetch<unknown>("/api/v1/mail/screener/decisions", {
    method: "POST",
    body: {
      account_id: input.accountId,
      sender_email: input.senderEmail,
      disposition: input.disposition,
    },
  });
}
