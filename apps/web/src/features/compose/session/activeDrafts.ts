/*
 * Reload survival: which compose file each intent was editing, kept in
 * localStorage so reopening the same intent resumes the draft instead of
 * starting a fresh session.
 */

import {
  restoreComposeSession,
  startComposeSession,
  rememberComposeBinding,
  refreshComposeSession,
  type ComposeSessionResponse,
} from "../api";

import {
  composeQueueKey,
  draftFingerprint,
  draftFromSession,
  type ComposeDraftState,
  type ComposeIntent,
} from "./composeDraft";
import { requestCoordinator } from "@/lib/requestCoordinator";

import {
  assertWorkspaceBinding,
  parseWorkspace,
  sameOwner,
  selectedWorkspace,
  type WorkspaceBinding,
} from "@/lib/workspaceOwner";

const activeDraftStorageKey = "mxr.compose.activeDrafts";
const canonicalDraftKey = (draftId: string, binding: WorkspaceBinding) =>
  `compose:draft:${encodeURIComponent(binding.target)}:${binding.workspace.workspace_id}:${binding.workspace.recovery_generation}:${draftId}`;

interface ActiveDraftEntry {
  snapshot?: ComposeDraftState;
  draftPath: string;
  recoverySourcePath?: string;
  preservedRecoveryPath?: string;
  accountId?: string;
  updatedAt: number;
}

export class UnownedPrivateDraftError extends Error {}

export async function loadInitialComposeSession(
  intent: ComposeIntent,
  explicitlyReopenCanonical = false,
): Promise<ComposeSessionResponse & { preservedRecoveryPath?: string }> {
  if (explicitlyReopenCanonical && intent.draftId) return restoreComposeSession(intent.draftId);
  const active = readActiveDraft(intent.key);
  if (active?.draftPath) {
    if (active.snapshot) {
      const original = active.snapshot;
      const binding = original.binding;
      const selected = selectedWorkspace();
      let settled = active;
      if (
        binding &&
        selected &&
        binding.target === selected.target &&
        sameOwner(binding.workspace, selected.workspace)
      ) {
        await requestCoordinator.settleCompose(
          composeQueueKey(original.draftPath, original.recoverySourcePath),
        );
        const latest = readActiveDraft(intent.key);
        if (
          latest?.snapshot?.binding?.target === binding.target &&
          sameOwner(latest.snapshot.binding.workspace, binding.workspace)
        )
          settled = latest;
        try {
          assertWorkspaceBinding(binding);
          const snapshot = settled.snapshot ?? original;
          rememberComposeBinding(snapshot.draftPath, binding);
          if (snapshot.recoverySourcePath) {
            const refreshed = await refreshComposeSession(
              snapshot.draftPath,
              snapshot.frontmatter.attach,
            );
            assertWorkspaceBinding(binding);
            const refreshedDraft = draftFromSession(
              {
                ...refreshed.session,
                binding,
                draftIntent: refreshed.session.draftIntent ?? snapshot.draftIntent,
              },
              snapshot.accountId,
            );
            // The bridge checked exact source references before relocating the owned copies.
            const sameText =
              draftFingerprint({
                ...refreshedDraft,
                frontmatter: { ...refreshedDraft.frontmatter, attach: snapshot.frontmatter.attach },
              }) === draftFingerprint(snapshot);
            if (
              sameText &&
              refreshedDraft.draftId === snapshot.draftId &&
              refreshedDraft.revision === snapshot.revision &&
              refreshedDraft.frontmatter.attach.length === snapshot.frontmatter.attach.length
            ) {
              return {
                ...refreshed,
                preservedRecoveryPath: snapshot.preservedRecoveryPath,
                session: {
                  ...refreshed.session,
                  binding,
                  accountId: snapshot.accountId,
                  draftIntent: refreshed.session.draftIntent ?? snapshot.draftIntent,
                },
              };
            }
          }
        } catch {
          // The local snapshot remains available if its original owner cannot be reached.
        }
      }
      const snapshot = settled.snapshot ?? original;
      if (snapshot.binding) rememberComposeBinding(snapshot.draftPath, snapshot.binding);
      return { retainedSnapshot: true, session: { ...snapshot, previewHtml: "" } };
    }
    throw new UnownedPrivateDraftError(
      "This retained private editor has no workspace owner. Its original reference is preserved; use an explicit saved-draft reopen/review before continuing.",
    );
  }
  if (intent.draftId) return restoreComposeSession(intent.draftId);
  const started = await startComposeSession(intent.kind, intent.messageId, intent.inviteAction);
  if (intent.kind !== "reply" && intent.kind !== "reply_all") return started;

  const canonicalId = started.session.draftId;
  if (!canonicalId) return started;
  const retained = findActiveDraftByCanonicalId(canonicalId, intent.key);
  if (!retained?.snapshot) return started;
  const snapshot = retained.snapshot;
  const binding = snapshot.binding;
  const selected = selectedWorkspace();
  if (
    !binding ||
    !selected ||
    binding.target !== selected.target ||
    !sameOwner(binding.workspace, selected.workspace)
  )
    return started;

  await requestCoordinator.settleCompose(
    composeQueueKey(snapshot.draftPath, snapshot.recoverySourcePath),
  );
  const latest = findActiveDraftByCanonicalId(canonicalId, intent.key)?.snapshot ?? snapshot;
  if (
    !latest.binding ||
    latest.binding.target !== binding.target ||
    !sameOwner(latest.binding.workspace, binding.workspace)
  )
    return started;
  rememberComposeBinding(latest.draftPath, binding);
  return { retainedSnapshot: true, session: { ...latest, previewHtml: "" } };
}

function findActiveDraftByCanonicalId(
  draftId: string,
  excludeKey: string,
): ActiveDraftEntry | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return undefined;
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) return undefined;
    const selected = selectedWorkspace();
    if (!selected) return undefined;
    const canonicalKey = canonicalDraftKey(draftId, {
      target: selected.target,
      workspace: selected.workspace,
    });
    const canonical = parsed[canonicalKey];
    const entries = [
      ...(canonical === undefined ? [] : [[canonicalKey, canonical] as const]),
      ...Object.entries(parsed),
    ];
    for (const [key, value] of entries) {
      if (key === excludeKey || !record(value) || typeof value.draftPath !== "string") continue;
      const entry = readActiveDraftEntry(value);
      const snapshot = entry.snapshot;
      if (
        snapshot?.draftId === draftId &&
        snapshot.binding?.target === selected.target &&
        sameOwner(snapshot.binding.workspace, selected.workspace)
      )
        return entry;
    }
  } catch {
    // A malformed or unreadable journal stays untouched; the resolved canonical file is safe.
  }
  return undefined;
}

function readActiveDraft(key: string): ActiveDraftEntry | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return undefined;
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed))
      throw new Error("Private compose journal is malformed; keep it for recovery");
    const selected = selectedWorkspace();
    const retainedKey = selected
      ? `${key}:retained:${selected.workspace.workspace_id}:${selected.workspace.recovery_generation}`
      : undefined;
    const entry =
      retainedKey && parsed[retainedKey] !== undefined ? parsed[retainedKey] : parsed[key];
    if (entry === undefined) return undefined;
    if (!record(entry) || typeof entry.draftPath !== "string")
      throw new Error("Retained compose identity is malformed");
    return readActiveDraftEntry(entry);
  } catch (error) {
    throw new Error("Cannot read retained private compose work; the journal was preserved", {
      cause: error,
    });
  }
}

function readActiveDraftEntry(entry: Record<string, unknown>): ActiveDraftEntry {
  if (typeof entry.draftPath !== "string")
    throw new Error("Retained compose identity is malformed");
  return {
    draftPath: entry.draftPath,
    updatedAt: typeof entry.updatedAt === "number" ? entry.updatedAt : 0,
    accountId: typeof entry.accountId === "string" ? entry.accountId : undefined,
    recoverySourcePath:
      typeof entry.recoverySourcePath === "string" ? entry.recoverySourcePath : undefined,
    preservedRecoveryPath:
      typeof entry.preservedRecoveryPath === "string" ? entry.preservedRecoveryPath : undefined,
    snapshot: entry.snapshot === undefined ? undefined : parseSnapshot(entry.snapshot),
  };
}

export function rememberActiveDraft(key: string, draft: ComposeDraftState): boolean {
  if (typeof window === "undefined") return false;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (!record(parsed)) return false;
    for (const entry of Object.values(parsed)) {
      if (!record(entry) || typeof entry.draftPath !== "string") return false;
      if (entry.snapshot !== undefined) parseSnapshot(entry.snapshot);
    }
    const prior = parsed[key];
    if (record(prior) && typeof prior.draftPath === "string") {
      const originalBinding =
        record(prior.snapshot) && record(prior.snapshot.binding)
          ? prior.snapshot.binding
          : undefined;
      const originalWorkspace = originalBinding && parseWorkspace(originalBinding.workspace);
      const ownerKey = originalWorkspace
        ? `${originalWorkspace.workspace_id}:${originalWorkspace.recovery_generation}`
        : `unowned:${prior.draftPath}`;
      if (
        !originalWorkspace ||
        !draft.binding ||
        originalBinding?.target !== draft.binding.target ||
        !sameOwner(originalWorkspace, draft.binding.workspace)
      ) {
        parsed[`${key}:retained:${ownerKey}`] = prior;
      }
    }
    const entry: ActiveDraftEntry = {
      snapshot: draft,
      draftPath: draft.draftPath,
      recoverySourcePath: draft.recoverySourcePath,
      preservedRecoveryPath: draft.preservedRecoveryPath,
      accountId: draft.accountId,
      updatedAt: Date.now(),
    };
    parsed[key] = entry;
    if (draft.draftId && draft.binding) {
      parsed[canonicalDraftKey(draft.draftId, draft.binding)] = entry;
      for (const [storedKey, stored] of Object.entries(parsed)) {
        if (!record(stored) || !record(stored.snapshot) || !record(stored.snapshot.binding))
          continue;
        const owner = parseWorkspace(stored.snapshot.binding.workspace);
        if (
          stored.snapshot.draftId === draft.draftId &&
          stored.snapshot.recoverySourcePath === draft.recoverySourcePath &&
          stored.snapshot.binding.target === draft.binding.target &&
          owner &&
          sameOwner(owner, draft.binding.workspace)
        )
          parsed[storedKey] = entry;
      }
    }
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
    return true;
  } catch {
    return false;
  }
}

export function forgetActiveDraft(key: string, binding?: WorkspaceBinding): boolean {
  if (typeof window === "undefined" || !binding) return false;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return true;
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) return false;
    const entry = parsed[key];
    if (record(entry) && record(entry.snapshot) && record(entry.snapshot.binding)) {
      const owner = parseWorkspace(entry.snapshot.binding.workspace);
      if (
        owner &&
        entry.snapshot.binding.target === binding.target &&
        sameOwner(owner, binding.workspace)
      )
        delete parsed[key];
    }
    delete parsed[
      `${key}:retained:${binding.workspace.workspace_id}:${binding.workspace.recovery_generation}`
    ];
    if (record(entry) && record(entry.snapshot) && typeof entry.snapshot.draftId === "string") {
      for (const [storedKey, stored] of Object.entries(parsed)) {
        if (
          !record(stored) ||
          !record(stored.snapshot) ||
          stored.snapshot.draftId !== entry.snapshot.draftId ||
          !record(stored.snapshot.binding)
        )
          continue;
        const owner = parseWorkspace(stored.snapshot.binding.workspace);
        if (
          owner &&
          stored.snapshot.binding.target === binding.target &&
          sameOwner(owner, binding.workspace)
        )
          delete parsed[storedKey];
      }
    }
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
    return true;
  } catch {
    return false;
  }
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
function parseSnapshot(value: unknown): ComposeDraftState {
  if (
    !record(value) ||
    typeof value.draftPath !== "string" ||
    typeof value.bodyMarkdown !== "string" ||
    typeof value.accountId !== "string" ||
    !record(value.frontmatter)
  )
    throw new Error("Private draft snapshot is malformed; retain the journal");
  const fm = value.frontmatter;
  if (
    typeof fm.to !== "string" ||
    typeof fm.cc !== "string" ||
    typeof fm.bcc !== "string" ||
    typeof fm.subject !== "string" ||
    typeof fm.from !== "string" ||
    !Array.isArray(fm.attach) ||
    !fm.attach.every((path: unknown) => typeof path === "string")
  )
    throw new Error("Private draft fields are malformed; retain the journal");
  let binding: ComposeDraftState["binding"];
  if (value.binding !== undefined) {
    if (!record(value.binding) || typeof value.binding.target !== "string")
      throw new Error("Retained draft target is malformed");
    const workspace = parseWorkspace(value.binding.workspace);
    if (!workspace) throw new Error("Retained draft workspace is malformed");
    binding = { target: value.binding.target, workspace };
  }
  return {
    binding,
    draftPath: value.draftPath,
    accountId: value.accountId,
    bodyMarkdown: value.bodyMarkdown,
    rawContent: typeof value.rawContent === "string" ? value.rawContent : "",
    kind: typeof value.kind === "string" ? value.kind : "recovery",
    issues: [],
    draftId: typeof value.draftId === "string" ? value.draftId : undefined,
    revision:
      typeof value.revision === "number" && Number.isSafeInteger(value.revision)
        ? value.revision
        : null,
    recoverySourcePath:
      typeof value.recoverySourcePath === "string" ? value.recoverySourcePath : undefined,
    preservedRecoveryPath:
      typeof value.preservedRecoveryPath === "string" ? value.preservedRecoveryPath : undefined,
    frontmatter: {
      to: fm.to,
      cc: fm.cc,
      bcc: fm.bcc,
      subject: fm.subject,
      from: fm.from,
      attach: fm.attach,
    },
    draftIntent:
      value.draftIntent === "new" ||
      value.draftIntent === "reply" ||
      value.draftIntent === "reply_all" ||
      value.draftIntent === "forward"
        ? value.draftIntent
        : undefined,
  };
}
