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
  draft_id: string;
  revision: number;
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
  retryRecovery?: boolean;
  accountId?: string;
  draftPath: string;
  frontmatter: ComposeFrontmatter;
  body: string;
  expectedRevision?: number | null;
}): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session/update", {
    method: "POST",
    body: {
      retry_recovery: input.retryRecovery ?? false,
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
  operation_id?: string;
  state?: "accepted";
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
  expectedRevision?: number | null,
  operationId?: string,
): Promise<ComposeSendResponse> {
  return apiFetch<ComposeSendResponse>("/api/v1/mail/compose/session/send", {
    method: "POST",
    body: {
      operation_id: operationId,
      draft_path: draftPath,
      account_id: accountId,
      expected_revision: expectedRevision,
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
  expectedRevision?: number | null,
): Promise<{ report: DraftSafetyReport }> {
  return apiFetch<{ report: DraftSafetyReport }>("/api/v1/mail/compose/session/safety-check", {
    method: "POST",
    body: { draft_path: draftPath, account_id: accountId, expected_revision: expectedRevision },
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

export async function supportsDraftAttachmentUpload(): Promise<boolean> {
  const status = await apiFetch<{ draft_attachment_upload_supported?: boolean }>(
    "/api/v1/admin/status",
  );
  return status.draft_attachment_upload_supported === true;
}

interface ComposeAttachmentOwnerRequest {
  draftPath: string;
  accountId: string;
}
interface ComposeAttachmentBeginRequest extends ComposeAttachmentOwnerRequest {
  operation: "begin";
  metadata: {
    id: string;
    draft_id: string;
    account_id: string;
    filename: string;
    mime_type: string;
    length_bytes: number;
    sha256: string;
  };
}
interface ComposeAttachmentChunkRequest extends ComposeAttachmentOwnerRequest {
  operation: "chunk";
  resourceId: string;
  offset: number;
  contentBase64: string;
}
interface ComposeAttachmentFinalizeRequest extends ComposeAttachmentOwnerRequest {
  operation: "finalize";
  resourceId: string;
  expectedRevision: number;
}
type ComposeAttachmentRequest =
  | ComposeAttachmentBeginRequest
  | ComposeAttachmentChunkRequest
  | ComposeAttachmentFinalizeRequest;
interface ComposeAttachmentOwnerResponse {
  draft_id: string;
  revision: number;
}
export function uploadComposeAttachment(
  input: ComposeAttachmentBeginRequest,
): Promise<ComposeAttachmentOwnerResponse>;
export function uploadComposeAttachment(
  input: ComposeAttachmentChunkRequest,
): Promise<{ ok: boolean }>;
export function uploadComposeAttachment(
  input: ComposeAttachmentFinalizeRequest,
): Promise<ComposeAttachmentUploadResponse>;
export function uploadComposeAttachment(
  input: ComposeAttachmentRequest,
): Promise<ComposeAttachmentOwnerResponse | { ok: boolean } | ComposeAttachmentUploadResponse> {
  const body = {
    draft_path: input.draftPath,
    account_id: input.accountId,
    operation: input.operation,
  };
  const declaration =
    input.operation === "begin"
      ? { metadata: input.metadata }
      : input.operation === "chunk"
        ? {
            resource_id: input.resourceId,
            offset: input.offset,
            content_base64: input.contentBase64,
          }
        : { resource_id: input.resourceId, expected_revision: input.expectedRevision };
  return apiFetch("/api/v1/mail/compose/session/attachment", {
    method: "POST",
    body: { ...body, ...declaration },
  });
}

export function fetchAccounts(): Promise<AccountsResponse> {
  return apiFetch<AccountsResponse>("/api/v1/platform/accounts");
}
