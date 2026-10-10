/*
 * Explicit save actions for a compose session: save locally, copy to the
 * provider as a server draft, and refresh from the file on disk. These are
 * fired from shortcuts, menus and editor ex commands that never await them,
 * so they report failures themselves and never reject.
 */

import { useMutation } from "@tanstack/react-query";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { toast } from "sonner";

import { refreshComposeSession, saveComposeSession } from "../api";
import {
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
    // Carry the local id so the daemon updates that row before making the
    // explicit provider copy. A new compose session has no local row to update.
    mutationFn: ({ draftPath, accountId }: { draftPath: string; accountId: string }) =>
      saveComposeSession(
        draftPath,
        accountId,
        draftRef.current?.draftId,
        draftRef.current?.revision,
      ),
  });

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
    try {
      await saveCurrentDraft();
      const current = draftRef.current;
      if (!current || !isCurrentDraftSaved(current)) {
        toast.error("Draft changed while saving", {
          description: "Save again before server draft.",
        });
        return;
      }
      const accountId = current.accountId;
      const draftPath = current.draftPath;
      await serverSave.mutateAsync({ draftPath, accountId });
    } catch (error) {
      toast.error("Server draft save failed", { description: errorMessage(error) });
      return;
    }
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
