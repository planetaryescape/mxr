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

import { DaemonUnavailableError } from "@/api/client";
import { useDaemonDown } from "@/lib/daemonAvailability";
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

const DAEMON_DOWN_NOTE = "mxr's daemon is stopped. Your text stays here and saves when it's back.";

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
  // Set once the draft file is gone (sent, scheduled, discarded) so the
  // unmount flush below never writes to a file that no longer exists.
  const sessionFinishedRef = useRef(false);

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
        async () => {
          const activeDraft = draftRef.current;
          const sameSession =
            activeDraft?.draftPath === snapshot.draftPath &&
            (snapshot.draftId == null || activeDraft.draftId === snapshot.draftId);
          const response = await updateSession.mutateAsync({
            accountId: snapshot.accountId,
            draftPath: snapshot.draftPath,
            frontmatter: snapshot.frontmatter,
            body: snapshot.body,
            expectedRevision: sameSession
              ? (activeDraft.revision ?? snapshot.revision)
              : snapshot.revision,
          });
          const latest = draftRef.current;
          if (
            latest?.draftPath === snapshot.draftPath &&
            (snapshot.draftId == null || latest.draftId === snapshot.draftId)
          ) {
            draftRef.current = {
              ...latest,
              draftId: response.session.draftId ?? undefined,
              revision: response.session.revision,
            };
            setDraft((active) =>
              active?.draftPath === snapshot.draftPath &&
              (snapshot.draftId == null || active.draftId === snapshot.draftId)
                ? {
                    ...active,
                    draftId: response.session.draftId ?? undefined,
                    revision: response.session.revision,
                  }
                : active,
            );
          }
          return response;
        },
      );
      if (result.status !== "committed") return undefined;
      const response = result.value;
      if (
        draftRef.current?.draftPath !== snapshot.draftPath ||
        (snapshot.draftId != null && draftRef.current.draftId !== snapshot.draftId)
      )
        return undefined;
      lastSavedFingerprintRef.current = snapshot.fingerprint;
      const latest = draftRef.current;
      if (latest && draftFingerprint(latest) === snapshot.fingerprint) {
        const next = draftFromSession(response.session, snapshot.accountId);
        setDraft(next);
        lastSavedFingerprintRef.current = draftFingerprint(next);
        setDirty(false);
        rememberActiveDraft(intentKey, next);
      } else if (latest) {
        setDraft({
          ...latest,
          draftId: response.session.draftId ?? undefined,
          revision: response.session.revision,
          issues: response.session.issues,
        });
      }
      setLastSavedAt(new Date());
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
      return response.session;
    } catch (error) {
      const message = errorMessage(error);
      if (draftRef.current?.draftPath === snapshot.draftPath) setSaveError(message);
      throw error;
    }
  }, [draftRef, intentKey, queryClient, setDirty, setDraft, updateSession]);

  // While the daemon is down nothing can save: say so and keep the text,
  // which stays dirty, so the debounce below runs again once it's back.
  const daemonDown = useDaemonDown();
  useEffect(() => {
    if (!daemonDown) setSaveError((shown) => (shown === DAEMON_DOWN_NOTE ? null : shown));
  }, [daemonDown]);

  useEffect(() => {
    if (!dirty || !draft) return;
    if (daemonDown) {
      setSaveError(DAEMON_DOWN_NOTE);
      return;
    }
    const handle = window.setTimeout(() => {
      void saveCurrentDraft().catch((error: Error) => {
        // The daemon just went away; the note above replaces this once
        // that's confirmed, and a toast per keystroke pause would nag.
        if (error instanceof DaemonUnavailableError) return;
        toast.error("Autosave failed", { description: error.message });
      });
    }, 3000);
    return () => window.clearTimeout(handle);
  }, [daemonDown, dirty, draft, saveCurrentDraft]);

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

  // Tearing the session down (closing the surface, switching reply target)
  // must not drop edits still inside the debounce window. The save is fired
  // on the way out; the coordinator keeps it ordered with any in-flight one.
  const saveOnUnmountRef = useRef(saveCurrentDraft);
  saveOnUnmountRef.current = saveCurrentDraft;
  useEffect(
    () => () => {
      if (sessionFinishedRef.current) return;
      void saveOnUnmountRef.current().catch((error: Error) => {
        toast.error("Couldn't save your last edits", { description: error.message });
      });
    },
    [],
  );

  function markSessionFinished() {
    sessionFinishedRef.current = true;
  }

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
    markSessionFinished,
  };
}
