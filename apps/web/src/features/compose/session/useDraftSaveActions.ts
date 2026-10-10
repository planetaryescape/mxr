/*
 * Explicit save actions for a compose session: save locally, copy to the
 * provider as a server draft, and refresh from the file on disk. These are
 * fired from shortcuts, menus and editor ex commands that never await them,
 * so they report failures themselves and never reject.
 */

import { useMutation } from "@tanstack/react-query";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { toast } from "sonner";

import { acceptedDraftRevision } from "./acceptedDraftRevision";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { refreshComposeSession, saveComposeSession } from "../api";
import {
  composeQueueKey,
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
}

export function useDraftSaveActions({
  draftRef,
  setDraft,
  setDirty,
  saveCurrentDraft,
  isCurrentDraftSaved,
  setSaveError,
  setLastSavedAt,
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
    try {
      await saveCurrentDraft();
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
      await requestCoordinator.queueComposeLatest(composeQueueKey(current.draftPath), async () => {
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
      });
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
      const response = await refreshComposeSession(current.draftPath);
      const next = draftFromSession(response.session, current.accountId);
      setDraft(next);
      setDirty(false);
      setSaveError(null);
      setLastSavedAt(new Date());
    } catch (error) {
      toast.error("Refresh failed", { description: errorMessage(error) });
      return;
    }
    toast.success("Draft refreshed");
  }

  return { handleSaveClick, handleServerSaveClick, handleRefreshClick };
}
