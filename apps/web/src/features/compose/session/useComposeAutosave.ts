/*
 * Autosave for a compose session: a trailing 3s debounce after the last
 * edit, flushed when the tab is hidden. Saves go through the request
 * coordinator so only the latest snapshot per draft file commits.
 */

import { useMutation, type QueryClient } from "@tanstack/react-query";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import { toast } from "sonner";

import { requestCoordinator } from "@/lib/requestCoordinator";
import { updateComposeSession } from "../api";
import { rememberActiveDraft } from "./activeDrafts";
import {
  captureSaveSnapshot,
  composeQueueKey,
  draftFingerprint,
  draftFromSession,
  errorMessage,
  type ComposeDraftState,
} from "./composeDraft";

interface ComposeAutosaveInput {
  intentKey: string;
  queryClient: QueryClient;
  draft: ComposeDraftState | null;
  draftRef: MutableRefObject<ComposeDraftState | null>;
  setDraft: Dispatch<SetStateAction<ComposeDraftState | null>>;
  dirty: boolean;
  setDirty: Dispatch<SetStateAction<boolean>>;
}

export function useComposeAutosave({
  intentKey,
  queryClient,
  draft,
  draftRef,
  setDraft,
  dirty,
  setDirty,
}: ComposeAutosaveInput) {
  const lastSavedFingerprintRef = useRef<string | null>(null);
  const [lastSavedAt, setLastSavedAt] = useState<Date | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const updateSession = useMutation({ mutationFn: updateComposeSession });

  const saveCurrentDraft = useCallback(async () => {
    const current = draftRef.current;
    if (!current) return undefined;
    const snapshot = captureSaveSnapshot(current);
    if (snapshot.fingerprint === lastSavedFingerprintRef.current) {
      setDirty(false);
      setSaveError(null);
      return undefined;
    }
    setSaveError(null);
    try {
      const result = await requestCoordinator.queueComposeLatest(
        composeQueueKey(snapshot.draftPath),
        async () =>
          await updateSession.mutateAsync({
            draftPath: snapshot.draftPath,
            frontmatter: snapshot.frontmatter,
            body: snapshot.body,
          }),
      );
      if (result.status !== "committed") return undefined;
      const response = result.value;
      lastSavedFingerprintRef.current = snapshot.fingerprint;
      const latest = draftRef.current;
      if (latest && draftFingerprint(latest) === snapshot.fingerprint) {
        const next = draftFromSession(response.session, snapshot.accountId);
        setDraft(next);
        lastSavedFingerprintRef.current = draftFingerprint(next);
        setDirty(false);
        rememberActiveDraft(intentKey, next);
      } else if (latest) {
        setDraft({ ...latest, issues: response.session.issues });
      }
      setLastSavedAt(new Date());
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
      return response.session;
    } catch (error) {
      const message = errorMessage(error);
      setSaveError(message);
      throw error;
    }
  }, [draftRef, intentKey, queryClient, setDirty, setDraft, updateSession]);

  useEffect(() => {
    if (!dirty || !draft) return;
    const handle = window.setTimeout(() => {
      void saveCurrentDraft().catch((error: Error) => {
        toast.error("Autosave failed", { description: error.message });
      });
    }, 3000);
    return () => window.clearTimeout(handle);
  }, [dirty, draft, saveCurrentDraft]);

  // Flush the autosave debounce the moment the tab is hidden — a closed tab
  // never comes back for the 3s timer.
  useEffect(() => {
    const flush = () => {
      if (document.visibilityState !== "hidden") return;
      if (!draftRef.current) return;
      void saveCurrentDraft().catch(() => {
        // beforeunload below still warns about the unsaved state.
      });
    };
    document.addEventListener("visibilitychange", flush);
    return () => document.removeEventListener("visibilitychange", flush);
  }, [draftRef, saveCurrentDraft]);

  function isCurrentDraftSaved(current: ComposeDraftState): boolean {
    return draftFingerprint(current) === lastSavedFingerprintRef.current;
  }

  return {
    saveCurrentDraft,
    isCurrentDraftSaved,
    lastSavedFingerprintRef,
    lastSavedAt,
    setLastSavedAt,
    saveError,
    setSaveError,
    saving: updateSession.isPending,
  };
}
