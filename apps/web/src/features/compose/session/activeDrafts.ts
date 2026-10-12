/*
 * Reload survival: which compose file each intent was editing, kept in
 * localStorage so reopening the same intent resumes the draft instead of
 * starting a fresh session.
 */

import {
  refreshComposeSession,
  restoreComposeSession,
  startComposeSession,
  rememberComposeBinding,
  type ComposeSessionResponse,
} from "../api";
import { requestCoordinator } from "@/lib/requestCoordinator";

import { composeQueueKey, type ComposeDraftState, type ComposeIntent } from "./composeDraft";

import { parseWorkspace } from "@/lib/workspaceOwner";

const activeDraftStorageKey = "mxr.compose.activeDrafts";

interface ActiveDraftEntry {
  snapshot?: ComposeDraftState;
  draftPath: string;
  recoverySourcePath?: string;
  preservedRecoveryPath?: string;
  accountId?: string;
  updatedAt: number;
}

export async function loadInitialComposeSession(
  intent: ComposeIntent,
): Promise<ComposeSessionResponse & { preservedRecoveryPath?: string }> {
  const active = readActiveDraft(intent.key);
  if (active?.draftPath) {
    try {
      if (active.snapshot) {
        if (active.snapshot.binding) rememberComposeBinding(active.snapshot.draftPath, active.snapshot.binding);
        // A private snapshot survives a missing original bridge without submission.
        const snapshot = active.snapshot;
        return { retainedSnapshot: true, session: {
          ...snapshot, draftId: snapshot.draftId, accountId: snapshot.accountId,
          bodyMarkdown: snapshot.bodyMarkdown, previewHtml: "", binding: snapshot.binding,
        } };
      }
      // A composer that just closed (focus mode moving on, a reply closed
      // and reopened) may still be saving this file: read it after that.
      await requestCoordinator.settleCompose(
        composeQueueKey(active.draftPath, active.recoverySourcePath),
      );
      const settled = readActiveDraft(intent.key) ?? active;
      if (intent.draftId && !settled.recoverySourcePath) {
        const restored = await restoreComposeSession(intent.draftId);
        return { ...restored, preservedRecoveryPath: settled.preservedRecoveryPath };
      }
      const refreshed = await refreshComposeSession(settled.draftPath);
      if (intent.draftId && refreshed.session.draftId !== intent.draftId)
        throw new Error("The recovered session belongs to another draft");
      // The refresh reads the file, which doesn't name the account; the
      // entry kept it. Without it the send and safety check have no account.
      return {
        ...refreshed,
        preservedRecoveryPath: settled.preservedRecoveryPath,
        session: {
          ...refreshed.session,
          accountId: refreshed.session.accountId ?? settled.accountId,
        },
      };
    } catch (error) {
      throw error;
    }
  }
  if (intent.draftId) return restoreComposeSession(intent.draftId);
  return startComposeSession(intent.kind, intent.messageId, intent.inviteAction);
}

function readActiveDraft(key: string): ActiveDraftEntry | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return undefined;
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) throw new Error("Private compose journal is malformed; keep it for recovery");
    const entry = parsed[key];
    if (entry === undefined) return undefined;
    if (!record(entry) || typeof entry.draftPath !== "string") throw new Error("Retained compose identity is malformed");
    return { draftPath: entry.draftPath, updatedAt: typeof entry.updatedAt === "number" ? entry.updatedAt : 0,
      accountId: typeof entry.accountId === "string" ? entry.accountId : undefined,
      recoverySourcePath: typeof entry.recoverySourcePath === "string" ? entry.recoverySourcePath : undefined,
      preservedRecoveryPath: typeof entry.preservedRecoveryPath === "string" ? entry.preservedRecoveryPath : undefined,
      snapshot: entry.snapshot === undefined ? undefined : parseSnapshot(entry.snapshot) };
  } catch (error) {
    throw new Error("Cannot read retained private compose work; the journal was preserved", { cause: error });
  }
}

export function rememberActiveDraft(key: string, draft: ComposeDraftState): boolean {
  if (typeof window === "undefined") return false;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (!record(parsed)) return false;
    parsed[key] = {
      snapshot: draft,
      draftPath: draft.draftPath,
      recoverySourcePath: draft.recoverySourcePath,
      preservedRecoveryPath: draft.preservedRecoveryPath,
      accountId: draft.accountId,
      updatedAt: Date.now(),
    };
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
    return true;
  } catch {
    return false;
  }
}

export function forgetActiveDraft(key: string): boolean {
  if (typeof window === "undefined") return false;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return true;
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) return false;
    delete parsed[key];
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
    return true;
  } catch {
    // Other private drafts remain recoverable even when this entry is unreadable.
    return false;
  }
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
function parseSnapshot(value: unknown): ComposeDraftState {
  if (!record(value) || typeof value.draftPath !== "string" || typeof value.bodyMarkdown !== "string" || typeof value.accountId !== "string" || !record(value.frontmatter)) throw new Error("Private draft snapshot is malformed; retain the journal");
  const fm = value.frontmatter;
  if (typeof fm.to !== "string" || typeof fm.cc !== "string" || typeof fm.bcc !== "string" || typeof fm.subject !== "string" || typeof fm.from !== "string" || !Array.isArray(fm.attach) || !fm.attach.every((path: unknown) => typeof path === "string")) throw new Error("Private draft fields are malformed; retain the journal");
  let binding: ComposeDraftState["binding"];
  if (value.binding !== undefined) {
    if (!record(value.binding) || typeof value.binding.target !== "string") throw new Error("Retained draft target is malformed");
    const workspace = parseWorkspace(value.binding.workspace);
    if (!workspace) throw new Error("Retained draft workspace is malformed");
    binding = { target: value.binding.target, workspace };
  }
  return { binding, draftPath: value.draftPath, accountId: value.accountId, bodyMarkdown: value.bodyMarkdown,
    rawContent: typeof value.rawContent === "string" ? value.rawContent : "", kind: typeof value.kind === "string" ? value.kind : "recovery", issues: [],
    draftId: typeof value.draftId === "string" ? value.draftId : undefined,
    revision: typeof value.revision === "number" && Number.isSafeInteger(value.revision) ? value.revision : null,
    recoverySourcePath: typeof value.recoverySourcePath === "string" ? value.recoverySourcePath : undefined,
    preservedRecoveryPath: typeof value.preservedRecoveryPath === "string" ? value.preservedRecoveryPath : undefined,
    frontmatter: { to: fm.to, cc: fm.cc, bcc: fm.bcc, subject: fm.subject, from: fm.from, attach: fm.attach } };
}
