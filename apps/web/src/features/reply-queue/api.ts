import { apiFetch } from "@/api/client";

export interface ReplyQueueMessage {
  id: string;
  thread_id: string;
  subject: string;
  snippet: string;
  date: string;
  from?: { name?: string | null; email: string };
}

export function fetchReplyQueue() {
  return apiFetch<{ messages: ReplyQueueMessage[] }>("/api/v1/mail/reply-later");
}

export function setReplyLater(messageId: string, flag: boolean) {
  return apiFetch<unknown>(`/api/v1/mail/reply-later/${encodeURIComponent(messageId)}`, {
    method: "POST",
    body: { flag },
  });
}
