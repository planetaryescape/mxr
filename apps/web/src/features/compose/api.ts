import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export interface ComposeFrontmatter {
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  from: string;
  attach: string[];
}

export interface ComposeIssue {
  severity: "error" | "warning" | string;
  message: string;
}

export type ComposeSession = components["schemas"]["ComposeSessionData"];
export type ComposeSessionResponse = components["schemas"]["ComposeSessionResponse"];

export interface RuntimeAccount {
  account_id: string;
  key?: string | null;
  name: string;
  email: string;
  provider_kind: string;
  sync_kind?: string | null;
  send_kind?: string | null;
  sync?: unknown;
  send?: unknown;
  enabled: boolean;
  is_default: boolean;
  capabilities?: {
    supports_send?: boolean;
    supports_local_drafts?: boolean;
    supports_server_drafts?: boolean;
  };
}

export interface AccountsResponse {
  accounts: RuntimeAccount[];
}

export interface ComposeAttachmentUploadResponse {
  path: string;
  filename: string;
  size_bytes: number;
}

export type ComposeKind = "new" | "reply" | "reply_all" | "forward";

/** Calendar invite response carried by an `invite_reply` compose session. */
export type InviteReplyAction = "accept" | "tentative" | "decline";

/** Kinds the bridge can open a compose session for. `invite_reply` is a
 * reply with an attached iCal REPLY; it is not a stored-draft intent. */
export type ComposeSessionKind = ComposeKind | "invite_reply";

export function startComposeSession(
  kind: ComposeSessionKind,
  messageId?: string,
  inviteAction?: InviteReplyAction,
): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session", {
    method: "POST",
    body: {
      kind,
      message_id: messageId,
      ...(kind === "invite_reply" && inviteAction ? { action: inviteAction } : {}),
    },
  });
}

export function restoreComposeSession(draftId: string): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session/restore", {
    method: "POST",
    body: { draft_id: draftId },
  });
}

export function refreshComposeSession(draftPath: string): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session/refresh", {
    method: "POST",
    body: { draft_path: draftPath },
  });
}

export function updateComposeSession(input: {
  accountId?: string;
  draftPath: string;
  frontmatter: ComposeFrontmatter;
  body: string;
  expectedRevision?: number | null;
}): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session/update", {
    method: "POST",
    body: {
      draft_path: input.draftPath,
      account_id: input.accountId,
      to: input.frontmatter.to,
      cc: input.frontmatter.cc,
      bcc: input.frontmatter.bcc,
      subject: input.frontmatter.subject,
      from: input.frontmatter.from,
      attach: input.frontmatter.attach,
      body: input.body,
      expected_revision: input.expectedRevision,
    },
  });
}

export interface ContactSuggestion {
  email: string;
  display_name?: string | null;
}

export async function fetchContactsAutocomplete(
  q: string,
  limit = 8,
): Promise<ContactSuggestion[]> {
  const params = new URLSearchParams({ q, limit: String(limit) });
  const data = await apiFetch<{ contacts?: ContactSuggestion[] }>(
    `/api/v1/mail/contacts/autocomplete?${params.toString()}`,
  );
  return data.contacts ?? [];
}

export interface ComposeSendResponse {
  ok: boolean;
  draft_id?: string;
  /** Local id of the message just sent (the daemon's send receipt). Needed
   * to set a no-reply reminder; null when the daemon only acknowledged. */
  message_id?: string | null;
}

export function sendComposeSession(
  draftPath: string,
  accountId: string,
  overrideSafetyToken?: string,
): Promise<ComposeSendResponse> {
  return apiFetch<ComposeSendResponse>("/api/v1/mail/compose/session/send", {
    method: "POST",
    body: {
      draft_path: draftPath,
      account_id: accountId,
      ...(overrideSafetyToken ? { override_safety_token: overrideSafetyToken } : {}),
    },
  });
}

export interface DraftSafetyIssue {
  code: string;
  severity: string;
  message: string;
  detail?: string | null;
  override_token?: string | null;
}

export interface DraftSafetyReport {
  allowed: boolean;
  verdict: string;
  issues: DraftSafetyIssue[];
  checked_at?: string | null;
}

export function checkComposeSafety(
  draftPath: string,
  accountId: string,
): Promise<{ report: DraftSafetyReport }> {
  return apiFetch<{ report: DraftSafetyReport }>("/api/v1/mail/compose/session/safety-check", {
    method: "POST",
    body: { draft_path: draftPath, account_id: accountId },
  });
}

export interface SuggestedCollaborator {
  email: string;
  display_name?: string | null;
  reason: string;
  confidence: string;
}

export function suggestComposeCollaborators(
  draftPath: string,
  accountId: string,
): Promise<{ suggestions: SuggestedCollaborator[] }> {
  return apiFetch<{ suggestions: SuggestedCollaborator[] }>(
    "/api/v1/mail/compose/session/collaborators",
    {
      method: "POST",
      body: { draft_path: draftPath, account_id: accountId },
    },
  );
}

/**
 * Store the open compose session as a local draft and schedule it, in one
 * call. The bridge parses the compose file the same way send does, so reply
 * headers, invite replies, attachments and the From alias carry over.
 */
export function scheduleComposeSession(input: {
  draftPath: string;
  accountId: string;
  draftId?: string;
  expectedRevision?: number | null;
  sendAt: Date;
}): Promise<components["schemas"]["ScheduledComposeResponse"]> {
  return apiFetch("/api/v1/mail/compose/session/schedule", {
    method: "POST",
    body: {
      draft_path: input.draftPath,
      account_id: input.accountId,
      draft_id: input.draftId,
      expected_revision: input.expectedRevision,
      send_at: input.sendAt.toISOString(),
    },
  });
}

/** "Remind me if no reply by `remindAt`" for a message already sent. */
export function setAutoReminder(sentMessageId: string, remindAt: Date): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/reminders", {
    method: "POST",
    body: { sent_message_id: sentMessageId, remind_at: remindAt.toISOString() },
  });
}

export function cancelAutoReminder(sentMessageId: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/v1/mail/reminders/${encodeURIComponent(sentMessageId)}`, {
    method: "DELETE",
  });
}

/** Cancel a scheduled send. The stored draft itself is kept. */
export function cancelScheduledSend(draftId: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/v1/mail/scheduled-sends/${encodeURIComponent(draftId)}`, {
    method: "DELETE",
  });
}

/**
 * Store the compose session as a draft.
 *
 * `draftId` is the stored draft this session was restored from, when it was
 * one — passing it updates that local draft in place before the provider copy.
 * Reusing the id is what stops an edit from becoming a second local row the
 * user then has to tell apart.
 */
export function saveComposeSession(
  draftPath: string,
  accountId: string,
  draftId?: string,
  expectedRevision?: number | null,
): Promise<components["schemas"]["SavedDraftResponse"]> {
  return apiFetch<components["schemas"]["SavedDraftResponse"]>(
    "/api/v1/mail/compose/session/save",
    {
      method: "POST",
      body: {
        draft_path: draftPath,
        account_id: accountId,
        save_to_server: true,
        expected_revision: expectedRevision,
        draft_id: draftId,
      },
    },
  );
}

export function discardComposeSession(
  draftPath: string,
  expectedRevision?: number | null,
): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/mail/compose/session/discard", {
    method: "POST",
    body: { draft_path: draftPath, expected_revision: expectedRevision },
  });
}

export function uploadComposeAttachment(input: {
  draftPath: string;
  filename: string;
  contentBase64: string;
}): Promise<ComposeAttachmentUploadResponse> {
  return apiFetch<ComposeAttachmentUploadResponse>("/api/v1/mail/compose/session/attachment", {
    method: "POST",
    body: {
      draft_path: input.draftPath,
      filename: input.filename,
      content_base64: input.contentBase64,
    },
  });
}

export function fetchAccounts(): Promise<AccountsResponse> {
  return apiFetch<AccountsResponse>("/api/v1/platform/accounts");
}
