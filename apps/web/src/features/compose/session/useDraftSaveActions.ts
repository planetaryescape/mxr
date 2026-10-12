/*
 * Explicit save actions for a compose session: save locally, copy to the
 * provider as a server draft, and refresh from the file on disk. These are
 * fired from shortcuts, menus and editor ex commands that never await them,
 * so they report failures themselves and never reject.
 */

import { useMutation } from "@tanstack/react-query";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { toast } from "sonner";

import { rememberActiveDraft } from "./activeDrafts";
import { acceptedDraftRevision } from "./acceptedDraftRevision";
import { requestCoordinator } from "@/lib/requestCoordinator";
import {
  refreshComposeSession,
  rememberComposeBinding,
  restoreComposeSession,
  saveComposeSession,
  updateComposeSession,
} from "../api";
import {
  composeQueueKey,
  draftFingerprint,
  draftFromSession,
  errorMessage,
  type ComposeDraftState,
  type ComposeIntent,
} from "./composeDraft";

interface DraftSaveActionsInput {
  intent: ComposeIntent;
  draftRef: MutableRefObject<ComposeDraftState | null>;
  setDraft: Dispatch<SetStateAction<ComposeDraftState | null>>;
  setDirty: Dispatch<SetStateAction<boolean>>;
  saveCurrentDraft: () => Promise<unknown>;
  isCurrentDraftSaved: (draft: ComposeDraftState) => boolean;
  setSaveError: Dispatch<SetStateAction<string | null>>;
  setLastSavedAt: Dispatch<SetStateAction<Date | null>>;
  lastSavedFingerprintRef: MutableRefObject<string | null>;
}

export function useDraftSaveActions({
  intent,
  draftRef,
  setDraft,
  setDirty,
  saveCurrentDraft,
  isCurrentDraftSaved,
  setSaveError,
  setLastSavedAt,
  lastSavedFingerprintRef,
}: DraftSaveActionsInput) {
  const serverSave = useMutation({
    mutationFn: (target: ComposeDraftState) =>
      saveComposeSession(target.draftPath, target.accountId, target.draftId, target.revision),
  });

  function adoptRevision(
    target: ComposeDraftState,
    draftId: string | null | undefined,
    revision: number | null | undefined,
  ) {
    const latest = draftRef.current;
    if (
      revision == null ||
      draftId !== target.draftId ||
      latest?.draftPath !== target.draftPath ||
      (target.draftId != null && latest.draftId !== target.draftId)
    )
      return;
    draftRef.current = { ...latest, draftId: draftId ?? latest.draftId, revision };
    setDraft((active) =>
      active?.draftPath === target.draftPath &&
      (target.draftId == null || active.draftId === target.draftId)
        ? { ...active, draftId: draftId ?? active.draftId, revision }
        : active,
    );
  }

  async function handleSaveClick() {
    const target = draftRef.current;
    if (!target) return;
    try {
      await saveCurrentDraft();
      const current = draftRef.current;
      if (
        !current ||
        current.draftId !== target.draftId ||
        composeQueueKey(current.draftPath, current.recoverySourcePath) !==
          composeQueueKey(target.draftPath, target.recoverySourcePath)
      )
        return;
      if (current.recoverySourcePath) {
        const response = await requestCoordinator.queueComposeLatest(
          composeQueueKey(current.draftPath, current.recoverySourcePath),
          () =>
            updateComposeSession({
              retryRecovery: true,
              draftPath: current.draftPath,
              accountId: current.accountId,
              expectedRevision: current.revision,
              frontmatter: { ...current.frontmatter, attach: [...current.frontmatter.attach] },
              body: current.bodyMarkdown,
            }),
        );
        if (response.status !== "committed") return;
        const saved = response.value.session;
        if (saved.draftId !== current.draftId || saved.revision == null || saved.recoverySourcePath)
          throw new Error(
            "The retry has no confirmed canonical draft response; your edits stay preserved",
          );
        const latest = draftRef.current;
        if (
          latest?.draftPath !== current.draftPath ||
          latest.draftId !== current.draftId ||
          draftFingerprint(latest) !== draftFingerprint(current)
        )
          throw new Error(
            "Your edits changed during the retry and remain preserved. Open the saved draft to continue.",
          );
        const next = {
          ...draftFromSession({ ...saved, binding: current.binding }, current.accountId),
          preservedRecoveryPath: current.draftPath,
        };
        if (next.binding) rememberComposeBinding(next.draftPath, next.binding);
        draftRef.current = next;
        lastSavedFingerprintRef.current = draftFingerprint(next);
        rememberActiveDraft(intent.key, next);
        setDraft(next);
        setDirty(false);
        setSaveError(null);
        setLastSavedAt(new Date());
      }
      const saved = draftRef.current;
      if (!saved || !isCurrentDraftSaved(saved)) {
        toast.error("Latest edits are not saved", {
          description: "Save again to save the current edits.",
        });
        return;
      }
    } catch (error) {
      toast.error("Save failed", { description: errorMessage(error) });
      return;
    }
    toast.success("Draft saved locally");
  }

  async function handleServerSaveClick() {
    const target = draftRef.current;
    if (!target) return;
    try {
      await saveCurrentDraft();
      const current = draftRef.current;
      if (
        current?.draftPath !== target.draftPath ||
        (target.draftId != null && current.draftId !== target.draftId)
      )
        return;
      if (!isCurrentDraftSaved(current)) {
        toast.error("Draft changed while saving", {
          description: "Save again before server draft.",
        });
        return;
      }
      await requestCoordinator.queueComposeLatest(
        composeQueueKey(current.draftPath, current.recoverySourcePath),
        async () => {
          const active = draftRef.current;
          const submitted =
            active?.draftPath === current.draftPath && active.draftId === current.draftId
              ? { ...current, revision: active.revision }
              : current;
          try {
            const response = await serverSave.mutateAsync(submitted);
            adoptRevision(current, response.draft_id, response.revision);
          } catch (error) {
            const accepted = acceptedDraftRevision(error);
            if (accepted) adoptRevision(current, accepted.draft_id, accepted.revision);
            throw error;
          }
        },
      );
    } catch (error) {
      toast.error("Server draft save failed", { description: errorMessage(error) });
      return;
    }
    if (draftRef.current?.draftPath === target.draftPath)
      toast.success("Draft copied to provider", {
        description: "The local mxr draft was preserved.",
      });
  }

  async function handleRefreshClick() {
    const current = draftRef.current;
    if (!current) return;
    try {
      if (current.recoverySourcePath) {
        await saveCurrentDraft();
        if (draftRef.current && draftFingerprint(draftRef.current) !== draftFingerprint(current))
          throw new Error(
            "Your text changed while preserving it. Open the saved draft again after saving.",
          );
      }
      const response =
        current.recoverySourcePath && current.draftId
          ? await restoreComposeSession(current.draftId, current.binding)
          : await refreshComposeSession(current.draftPath);
      if (
        draftRef.current?.draftPath !== current.draftPath ||
        draftRef.current.draftId !== current.draftId ||
        draftFingerprint(draftRef.current) !== draftFingerprint(current)
      )
        throw new Error(
          "Your text changed while opening the draft. Save it before opening another version.",
        );
      const next = {
        ...draftFromSession(response.session, current.accountId),
        preservedRecoveryPath: current.recoverySourcePath
          ? current.draftPath
          : current.preservedRecoveryPath,
      };
      draftRef.current = next;
      lastSavedFingerprintRef.current = next.recoverySourcePath ? null : draftFingerprint(next);
      rememberActiveDraft(intent.key, next);
      setDraft(next);
      setDirty(false);
      setSaveError(
        next.preservedRecoveryPath
          ? `Your rejected edits remain in ${next.preservedRecoveryPath}`
          : null,
      );
      setLastSavedAt(new Date());
    } catch (error) {
      toast.error("Refresh failed", { description: errorMessage(error) });
      return;
    }
    toast.success("Draft refreshed");
  }

  async function handleRecoveryClick() {
    const current = draftRef.current;
    if (!current?.preservedRecoveryPath) return;
    try {
      await saveCurrentDraft();
      if (
        draftRef.current?.draftPath !== current.draftPath ||
        draftFingerprint(draftRef.current) !== draftFingerprint(current)
      )
        return;
      const response = await refreshComposeSession(current.preservedRecoveryPath);
      if (
        draftRef.current?.draftPath !== current.draftPath ||
        draftRef.current.draftId !== current.draftId ||
        draftFingerprint(draftRef.current) !== draftFingerprint(current)
      )
        throw new Error(
          "Your text changed while opening preserved edits. Save it before opening another version.",
        );
      if (!response.session.recoverySourcePath || response.session.draftId !== current.draftId)
        throw new Error("The preserved session belongs to another draft");
      const next = draftFromSession(response.session, current.accountId);
      draftRef.current = next;
      lastSavedFingerprintRef.current = null;
      setDraft(next);
      setDirty(false);
      rememberActiveDraft(intent.key, next);
      setSaveError("These edits are preserved locally. Use Retry or Open saved draft to continue.");
    } catch (error) {
      toast.error("Recovery could not be opened", { description: errorMessage(error) });
    }
  }

  return { handleSaveClick, handleServerSaveClick, handleRefreshClick, handleRecoveryClick };
}
