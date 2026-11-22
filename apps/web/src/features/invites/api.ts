import { apiFetch } from "@/api/client";
import type { CalendarMetadataView } from "@/features/mailbox/types";

export interface CalendarInviteData {
  id: string;
  account_id: string;
  message_id: string;
  metadata: CalendarMetadataView;
  created_at: number;
  updated_at: number;
}

export function fetchInvites(account?: string) {
  const query = new URLSearchParams({ limit: "200" });
  if (account) query.set("account", account);
  return apiFetch<{ invites: CalendarInviteData[] }>(`/api/v1/mail/invites?${query.toString()}`);
}
