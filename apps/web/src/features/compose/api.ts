import { apiFetch } from "@/api/client";

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

export interface ComposeSession {
  draftPath: string;
  rawContent: string;
  frontmatter: ComposeFrontmatter;
  bodyMarkdown: string;
  previewHtml?: string;
  issues: ComposeIssue[];
  accountId?: string;
  kind?: string;
  editorCommand?: string;
  cursorLine?: number;
}

export interface ComposeSessionResponse {
  session: ComposeSession;
}

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

export function startComposeSession(
  kind: ComposeKind,
  messageId?: string,
): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session", {
    method: "POST",
    body: { kind, message_id: messageId },
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
  draftPath: string;
  frontmatter: ComposeFrontmatter;
  body: string;
}): Promise<ComposeSessionResponse> {
  return apiFetch<ComposeSessionResponse>("/api/v1/mail/compose/session/update", {
    method: "POST",
    body: {
      draft_path: input.draftPath,
      to: input.frontmatter.to,
      cc: input.frontmatter.cc,
      bcc: input.frontmatter.bcc,
      subject: input.frontmatter.subject,
      from: input.frontmatter.from,
      attach: input.frontmatter.attach,
      body: input.body,
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

export function sendComposeSession(
  draftPath: string,
  accountId: string,
  overrideSafetyToken?: string,
): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/mail/compose/session/send", {
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

export interface DraftAddress {
  name: string | null;
  email: string;
}

/** Payload for `POST /drafts/save-local` (the daemon `Draft` shape). Scheduled
 * sends operate on stored drafts, so the compose session is materialised into
 * one before scheduling. */
export interface LocalDraftPayload {
  id: string;
  account_id: string;
  intent: ComposeKind;
  to: DraftAddress[];
  cc: DraftAddress[];
  bcc: DraftAddress[];
  subject: string;
  body_markdown: string;
  attachments: string[];
  created_at: string;
  updated_at: string;
}

export function saveLocalDraft(draft: LocalDraftPayload): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/drafts/save-local", {
    method: "POST",
    body: draft,
  });
}

export function createScheduledSend(draftId: string, sendAt: Date): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/scheduled-sends", {
    method: "POST",
    body: { draft_id: draftId, send_at: sendAt.toISOString() },
  });
}

export function saveComposeSession(draftPath: string, accountId: string): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/mail/compose/session/save", {
    method: "POST",
    body: { draft_path: draftPath, account_id: accountId },
  });
}

export function discardComposeSession(draftPath: string): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/mail/compose/session/discard", {
    method: "POST",
    body: { draft_path: draftPath },
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
