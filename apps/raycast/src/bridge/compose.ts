import { request } from "./client";
import type {
  ComposeAssistAction,
  ComposeAssistProposal,
  ComposeAssistRequest,
  ComposeSession,
  Reply,
  SafetyReport,
  Workspace,
  ComposeModelChoice,
  ComposeModelCatalogue,
} from "./types";

// Compose sessions are private daemon work, bound to the archive workspace
// they were opened in; every call carries that binding, as the web app does.

export type ReplyKind = "reply" | "reply_all";

export interface ReplyDraft {
  session: ComposeSession;
  workspace: Workspace;
}

export async function composeModelChoices(): Promise<ComposeModelChoice[]> {
  const response = await request<{ models: ComposeModelCatalogue }>("/api/v1/mail/compose/models");
  return response.models.choices;
}

async function workspace(): Promise<Workspace> {
  const result = await request<{ workspace?: Workspace }>(
    "/api/v1/mail/archive-operations/workspace",
  );
  if (!result.workspace?.workspace_id) {
    throw new Error("This mxr daemon cannot open private compose work; upgrade mxr.");
  }
  return result.workspace;
}

function owned<T>(path: string, body: unknown, binding: Workspace): Promise<T> {
  return request<T>(path, { body, headers: { "x-mxr-workspace": JSON.stringify(binding) } });
}

/** Open a reply to `messageId` with recipients, subject and threading headers filled in; the body starts empty, as in the web composer. */
export async function startReply(messageId: string, kind: ReplyKind): Promise<ReplyDraft> {
  const binding = await workspace();
  const { session } = await owned<{ session: ComposeSession }>(
    "/api/v1/mail/compose/session",
    { kind, message_id: messageId },
    binding,
  );
  return { session, workspace: binding };
}

/** Ask the daemon for a proof against the exact text in this unsaved reply. */
export async function assistReply(
  draft: ReplyDraft,
  messageId: string,
  kind: ReplyKind,
  text: string,
  options: {
    action?: ComposeAssistAction;
    revise?: string;
    instruction?: string;
    model?: string;
  } = {},
): Promise<ComposeAssistProposal> {
  const body: ComposeAssistRequest = {
    draft: { kind: "reply", message_id: messageId, all: kind === "reply_all" },
    text,
  };
  if (options.action) body.action = options.action;
  if (options.revise) body.revise = options.revise;
  if (options.instruction) body.instruction = options.instruction;
  if (options.model) body.model = options.model;
  const { proposal } = await owned<{ proposal: ComposeAssistProposal }>(
    "/api/v1/mail/compose/assist",
    body,
    draft.workspace,
  );
  return proposal;
}

/** Write the body the user typed into the draft and return the new revision. */
export async function writeBody(draft: ReplyDraft, body: string): Promise<ReplyDraft> {
  const { frontmatter } = draft.session;
  const { session } = await owned<{ session: ComposeSession }>(
    "/api/v1/mail/compose/session/update",
    {
      draft_path: draft.session.draftPath,
      account_id: draft.session.accountId,
      to: frontmatter.to,
      cc: frontmatter.cc,
      bcc: frontmatter.bcc,
      subject: frontmatter.subject,
      from: frontmatter.from,
      attach: frontmatter.attach,
      body,
      expected_revision: draft.session.revision,
    },
    draft.workspace,
  );
  return { ...draft, session };
}

/** The send preview: the same checks the daemon enforces when it sends. */
export async function checkSafety(draft: ReplyDraft): Promise<SafetyReport> {
  const { report } = await owned<{ report: SafetyReport }>(
    "/api/v1/mail/compose/session/safety-check",
    {
      draft_path: draft.session.draftPath,
      account_id: accountOf(draft),
      expected_revision: draft.session.revision,
    },
    draft.workspace,
  );
  return report;
}

export async function sendReply(draft: ReplyDraft, overrideToken?: string): Promise<void> {
  await owned(
    "/api/v1/mail/compose/session/send",
    {
      draft_path: draft.session.draftPath,
      account_id: accountOf(draft),
      expected_revision: draft.session.revision,
      operation_id: crypto.randomUUID(),
      ...(overrideToken ? { override_safety_token: overrideToken } : {}),
    },
    draft.workspace,
  );
}

/**
 * Hand the reply to the daemon to send at `sendAt`. The daemon owns the
 * delay, so the undo window survives Raycast closing. Returns the stored
 * draft's id, which cancels it.
 */
export async function scheduleReply(draft: ReplyDraft, sendAt: Date): Promise<string> {
  const result = await owned<{ draft_id?: string }>(
    "/api/v1/mail/compose/session/schedule",
    {
      draft_path: draft.session.draftPath,
      account_id: accountOf(draft),
      expected_revision: draft.session.revision,
      send_at: sendAt.toISOString(),
    },
    draft.workspace,
  );
  if (!result.draft_id) throw new Error("mxr scheduled the reply but did not return its draft.");
  return result.draft_id;
}

/** Stop a scheduled reply; it stays in mxr as a draft. */
export async function cancelScheduled(draftId: string): Promise<void> {
  await request(`/api/v1/mail/scheduled-sends/${encodeURIComponent(draftId)}`, {
    method: "DELETE",
  });
}

/**
 * Store the reply as a local mxr draft so the web composer can pick it up
 * where Raycast left off. Not pushed to the provider's drafts.
 */
export async function saveForWeb(draft: ReplyDraft): Promise<string> {
  const result = await owned<{ draft_id?: string }>(
    "/api/v1/mail/compose/session/save",
    {
      draft_path: draft.session.draftPath,
      account_id: accountOf(draft),
      save_to_server: false,
      expected_revision: draft.session.revision,
      draft_id: draft.session.draftId ?? undefined,
    },
    draft.workspace,
  );
  if (!result.draft_id) throw new Error("mxr saved the reply but did not return its draft.");
  return result.draft_id;
}

/** Throw the draft away, so an abandoned reply leaves nothing behind. */
export async function discardReply(draft: ReplyDraft): Promise<void> {
  await owned(
    "/api/v1/mail/compose/session/discard",
    { draft_path: draft.session.draftPath, expected_revision: draft.session.revision },
    draft.workspace,
  );
}

/** A suggested body in the user's own voice; nothing is saved. */
export async function suggestBody(threadId: string, instruction: string): Promise<string> {
  const suggestion = await request<Reply<"DraftSuggestion">>("/api/v1/mail/drafts/compose", {
    body: { instruction, thread_id: threadId },
  });
  return suggestion.body;
}

function accountOf(draft: ReplyDraft): string {
  const account = draft.session.accountId;
  if (!account) throw new Error("mxr did not say which account this reply sends from.");
  return account;
}
