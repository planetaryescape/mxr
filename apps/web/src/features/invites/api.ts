import { apiFetch } from "@/api/client";
import type { CalendarMetadataView } from "@/features/mailbox/types";

/// One row of the calendar-invites list. Mirrors the daemon's
/// `CalendarInviteData` (protocol) returned by `GET /api/v1/mail/invites`.
export interface CalendarInviteData {
  id: string;
  account_id: string;
  message_id: string;
  metadata: CalendarMetadataView;
  created_at: number;
  updated_at: number;
}

export function fetchInvites() {
  return apiFetch<{ invites: CalendarInviteData[] }>(
    "/api/v1/mail/invites?limit=200",
  );
}
