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
import { DaemonUnavailableError } from "@/api/client";
import { requestCoordinator } from "@/lib/requestCoordinator";

import { composeQueueKey, type ComposeDraftState, type ComposeIntent } from "./composeDraft";

import { WorkspaceConflictError } from "@/lib/workspaceOwner";

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
      if (error instanceof DaemonUnavailableError || error instanceof WorkspaceConflictError) throw error;
      forgetActiveDraft(intent.key);
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
    const parsed = JSON.parse(raw) as Record<string, ActiveDraftEntry>;
    return parsed[key];
  } catch {
    return undefined;
  }
}

export function rememberActiveDraft(key: string, draft: ComposeDraftState) {
  if (typeof window === "undefined") return;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    const parsed = raw ? (JSON.parse(raw) as Record<string, ActiveDraftEntry>) : {};
    parsed[key] = {
      snapshot: draft,
      draftPath: draft.draftPath,
      recoverySourcePath: draft.recoverySourcePath,
      preservedRecoveryPath: draft.preservedRecoveryPath,
      accountId: draft.accountId,
      updatedAt: Date.now(),
    };
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
  } catch {
    // Reload survival is best-effort only.
  }
}

export function forgetActiveDraft(key: string) {
  if (typeof window === "undefined") return;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return;
    const parsed = JSON.parse(raw) as Record<string, ActiveDraftEntry>;
    delete parsed[key];
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
  } catch {
    // Corrupt entry: drop it. Storage itself may be unavailable (private
    // mode, disabled site data), which must never break a send or schedule.
    try {
      window.localStorage.removeItem(activeDraftStorageKey);
    } catch {
      // Nothing to clean up without storage.
    }
  }
}
