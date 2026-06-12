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
  return apiFetch<ShellResponse>("/api/v1/client/shell");
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

export interface ThreadBriefing {
  thread_id: string;
  body_markdown: string;
  citations: { message_id?: string; subject?: string; date?: string }[];
  generated_at: string;
  from_cache: boolean;
}

export function getThreadBriefing(input: {
  threadId: string;
  refresh?: boolean;
}): Promise<{ briefing: ThreadBriefing }> {
  const query = input.refresh ? "?refresh=true" : "";
  return apiFetch<{ briefing: ThreadBriefing }>(
    `/api/v1/mail/threads/${encodeURIComponent(input.threadId)}/briefing${query}`,
  );
}

export function getRecipientBriefing(input: {
  accountId: string;
  email: string;
  refresh?: boolean;
}): Promise<{ briefing: ThreadBriefing }> {
  const query = new URLSearchParams({ account_id: input.accountId, email: input.email });
  if (input.refresh) query.set("refresh", "true");
  return apiFetch<{ briefing: ThreadBriefing }>(
    `/api/v1/mail/contacts/briefing?${query.toString()}`,
  );
}

export interface ExpertSuggestion {
  email: string;
  display_name?: string | null;
  reason: string;
  answered_thread_count: number;
  evidence_msg_ids: string[];
}

export function findExpert(input: {
  accountId: string;
  query: string;
  limit?: number;
}): Promise<{ experts: ExpertSuggestion[] }> {
  const query = new URLSearchParams({ account_id: input.accountId, query: input.query });
  if (input.limit) query.set("limit", String(input.limit));
  return apiFetch<{ experts: ExpertSuggestion[] }>(
    `/api/v1/mail/contacts/expert?${query.toString()}`,
  );
}

export function getRelationshipProfile(input: {
  accountId: string;
  email: string;
}): Promise<unknown> {
  const query = new URLSearchParams({ account_id: input.accountId, email: input.email });
  return apiFetch<unknown>(`/api/v1/mail/relationship?${query.toString()}`);
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

export function renameLabel(input: {
  oldName: string;
  newName: string;
  accountId?: string;
}): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/labels/rename", {
    method: "POST",
    body: {
      old: input.oldName,
      new: input.newName,
      ...(input.accountId ? { account_id: input.accountId } : {}),
    },
  });
}

export function deleteLabel(input: { name: string; accountId?: string }): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/labels/delete", {
    method: "POST",
    body: {
      name: input.name,
      ...(input.accountId ? { account_id: input.accountId } : {}),
    },
  });
}

export function moveMessagesToLabel(
  messageIds: string[],
  targetLabel: string,
): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/move", {
    method: "POST",
    body: { message_ids: messageIds, target_label: targetLabel },
  });
}

export function routeMessages(input: {
  messageIds: string[];
  toLabel: string;
  fromQueueLabel: string;
  archive?: boolean;
  dryRun?: boolean;
}): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/route", {
    method: "POST",
    body: {
      message_ids: input.messageIds,
      to_label: input.toLabel,
      from_queue_label: input.fromQueueLabel,
      archive: input.archive ?? false,
      dry_run: input.dryRun ?? false,
    },
  });
}

export function readAndArchiveMessages(messageIds: string[]): Promise<MutationResponse> {
  return apiFetch<MutationResponse>("/api/v1/mail/mutations/read-and-archive", {
    method: "POST",
    body: { message_ids: messageIds },
  });
}

export function unsubscribeFromSender(input: {
  messageId: string;
  archive: boolean;
}): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/actions/unsubscribe", {
    method: "POST",
    body: { message_id: input.messageId, archive: input.archive },
  });
}

export interface UnsubscribePurgeResponse {
  ok: boolean;
  result?: {
    address: string;
    status: string;
    method?: unknown;
    message_count: number;
    archived_count: number;
    mutation_id?: string | null;
    error?: string | null;
  };
}

export function unsubscribeAndClearSender(input: {
  address: string;
  accountId?: string;
  dryRun?: boolean;
  archiveOnNoMethod?: boolean;
}): Promise<UnsubscribePurgeResponse> {
  return apiFetch<UnsubscribePurgeResponse>("/api/v1/mail/actions/unsubscribe-purge", {
    method: "POST",
    body: {
      address: input.address,
      account_id: input.accountId,
      dry_run: input.dryRun ?? false,
      archive_on_no_method: input.archiveOnNoMethod ?? false,
    },
  });
}

export interface DraftAssistResponse {
  body?: string;
  draft?: string;
  message?: string;
  inferred_register?: "casual" | "neutral" | "formal" | null;
  inferred_length?: "short" | "medium" | "long" | null;
  context_note?: string | null;
}

export function draftAssistThread(input: {
  threadId: string;
  instruction: string;
  register?: "casual" | "neutral" | "formal";
  lengthHint?: "short" | "medium" | "long";
}): Promise<DraftAssistResponse> {
  return apiFetch<DraftAssistResponse>("/api/v1/mail/drafts/compose", {
    method: "POST",
    body: {
      thread_id: input.threadId,
      instruction: input.instruction,
      ...(input.register ? { register: input.register } : {}),
      ...(input.lengthHint ? { length_hint: input.lengthHint } : {}),
    },
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
