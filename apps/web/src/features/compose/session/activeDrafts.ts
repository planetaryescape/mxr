/*
 * Reload survival: which compose file each intent was editing, kept in
 * localStorage so reopening the same intent resumes the draft instead of
 * starting a fresh session.
 */

import { refreshComposeSession, restoreComposeSession, startComposeSession } from "../api";
import type { ComposeDraftState, ComposeIntent } from "./composeDraft";

const activeDraftStorageKey = "mxr.compose.activeDrafts";

interface ActiveDraftEntry {
  draftPath: string;
  accountId?: string;
  updatedAt: number;
}

export async function loadInitialComposeSession(intent: ComposeIntent) {
  if (intent.draftId) return restoreComposeSession(intent.draftId);
  const active = readActiveDraft(intent.key);
  if (active?.draftPath) {
    try {
      return await refreshComposeSession(active.draftPath);
    } catch {
      forgetActiveDraft(intent.key);
    }
  }
  return startComposeSession(intent.kind, intent.messageId);
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
    parsed[key] = { draftPath: draft.draftPath, accountId: draft.accountId, updatedAt: Date.now() };
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
    window.localStorage.removeItem(activeDraftStorageKey);
  }
}
