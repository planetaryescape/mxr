import { apiFetch } from "@/api/client";

import type { MailboxResponse, MutationResponse, ShellResponse, ThreadResponse } from "./types";

export interface SnoozePreset {
  id?: string;
  name?: string;
  label?: string;
  wakeAt?: string;
  wake_at?: string;
}

export interface MailboxLensParams {
  lens_kind: "inbox" | "all_mail" | "label" | "saved_search" | "subscription";
  label_id?: string;
  saved_search?: string;
  sender_email?: string;
}

export interface MailboxQueryParams extends MailboxLensParams {
  limit?: number;
  offset?: number;
  view?: "threads" | "messages";
}

export function mailboxKey(params: MailboxQueryParams) {
  return ["mailbox", params] as const;
}

export const shellKey = ["shell"] as const;

export async function fetchShell(): Promise<ShellResponse> {
  return apiFetch<ShellResponse>("/api/v1/desktop/shell");
}

export async function fetchMailbox(params: MailboxQueryParams): Promise<MailboxResponse> {
  const query = new URLSearchParams();
  query.set("lens_kind", params.lens_kind);
  query.set("view", params.view ?? "threads");
  query.set("limit", String(params.limit ?? 200));
  query.set("offset", String(params.offset ?? 0));
  if (params.label_id) query.set("label_id", params.label_id);
  if (params.saved_search) query.set("saved_search", params.saved_search);
  if (params.sender_email) query.set("sender_email", params.sender_email);
  return apiFetch<MailboxResponse>(`/api/v1/mail/mailbox?${query.toString()}`);
}

export function fetchThread(threadId: string): Promise<ThreadResponse> {
  return apiFetch<ThreadResponse>(`/api/v1/mail/threads/${threadId}`);
}

const SUMMARY_TIMEOUT_MS = 125_000;

export async function summarizeThread(threadId: string): Promise<unknown> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), SUMMARY_TIMEOUT_MS);
  try {
    return await apiFetch<unknown>(
      `/api/v1/mail/threads/${encodeURIComponent(threadId)}/summarize`,
      {
        method: "POST",
        signal: controller.signal,
      },
    );
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error(
        "Summary timed out after 125 seconds. Check the local model or try a smaller thread.",
        {
          cause: error,
        },
      );
    }
    throw error;
  } finally {
    window.clearTimeout(timeout);
  }
}

export function fetchSenderProfile(input: { accountId: string; email: string }): Promise<unknown> {
  const query = new URLSearchParams({ account_id: input.accountId, email: input.email });
  return apiFetch<unknown>(`/api/v1/mail/sender?${query.toString()}`);
}

export function listCommitments(input: {
  accountId: string;
  email?: string;
  status?: "open" | "resolved" | "expired";
}): Promise<unknown> {
  const query = new URLSearchParams({ account_id: input.accountId });
  if (input.email) query.set("email", input.email);
  if (input.status) query.set("status", input.status);
  return apiFetch<unknown>(`/api/v1/mail/commitments?${query.toString()}`);
}

export function resolveCommitment(commitmentId: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/v1/mail/commitments/${encodeURIComponent(commitmentId)}/resolve`, {
    method: "POST",
  });
}

interface AttachmentActionInput {
  messageId: string;
  attachmentId: string;
}

interface AttachmentActionResponse {
  file?: string;
}

export function openAttachment(input: AttachmentActionInput): Promise<AttachmentActionResponse> {
  return apiFetch<AttachmentActionResponse>("/api/v1/mail/attachments/open", {
    method: "POST",
    body: { message_id: input.messageId, attachment_id: input.attachmentId },
  });
}

export function downloadAttachment(
  input: AttachmentActionInput,
): Promise<AttachmentActionResponse> {
  return apiFetch<AttachmentActionResponse>("/api/v1/mail/attachments/download", {
    method: "POST",
    body: { message_id: input.messageId, attachment_id: input.attachmentId },
  });
}

export function archiveMessages(messageIds: string[]): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/archive", {
    method: "POST",
    body: { message_ids: messageIds },
  });
}

export function trashMessages(messageIds: string[]): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/trash", {
    method: "POST",
    body: { message_ids: messageIds },
  });
}

export function spamMessages(messageIds: string[]): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/spam", {
    method: "POST",
    body: { message_ids: messageIds },
  });
}

export function starMessages(messageIds: string[], starred: boolean): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/star", {
    method: "POST",
    body: { message_ids: messageIds, starred },
  });
}

export function markReadMessages(messageIds: string[], read: boolean): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/read", {
    method: "POST",
    body: { message_ids: messageIds, read },
  });
}

export function modifyLabels(
  messageIds: string[],
  add: string[],
  remove: string[],
): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/labels", {
    method: "POST",
    body: { message_ids: messageIds, add, remove },
  });
}

export function undoMutation(mutationId: string): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/mutations/undo", {
    method: "POST",
    body: { mutation_id: mutationId },
  });
}

export function fetchSnoozePresets(): Promise<{ presets: SnoozePreset[] }> {
  return apiFetch<{ presets: SnoozePreset[] }>("/api/v1/mail/actions/snooze/presets");
}

export function snoozeMessage(input: { messageId: string; until: string }): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/actions/snooze", {
    method: "POST",
    body: { message_id: input.messageId, until: input.until },
  });
}

export async function snoozeMessages(messageIds: string[], until: string): Promise<unknown[]> {
  return Promise.all(messageIds.map((messageId) => snoozeMessage({ messageId, until })));
}
