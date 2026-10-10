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
import { updateComposeSession, type ComposeSessionResponse, type ComposeFrontmatter } from "../api";
import { acceptedDraftRevision, recoveredComposeSession } from "./acceptedDraftRevision";
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
  const recoveryTargetRef = useRef<
    | {
        sourcePath: string;
        draftPath: string;
        draftId?: string;
        revision?: number | null;
        attachmentPaths?: Map<string, string>;
      }
    | undefined
  >(undefined);
  const pendingSavesRef = useRef(0);
  const currentIntentKeyRef = useRef(intentKey);
  currentIntentKeyRef.current = intentKey;
  const lastSavedFingerprintRef = useRef<string | null>(null);
  const preservedFingerprintRef = useRef<string | null>(null);
  const attemptedFingerprintRef = useRef<string | null>(null);
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
    const queueKey = composeQueueKey(snapshot.draftPath, snapshot.recoverySourcePath);
    const sourcePath = snapshot.recoverySourcePath ?? snapshot.draftPath;
    const unchanged = current.recoverySourcePath
      ? preservedFingerprintRef.current === `${current.draftPath}:${snapshot.fingerprint}`
      : snapshot.fingerprint === lastSavedFingerprintRef.current;
    if (unchanged && pendingSavesRef.current === 0) {
      setDirty(false);
      if (!current.recoverySourcePath) setSaveError(null);
      return undefined;
    }
    if (!current.recoverySourcePath) setSaveError(null);
    attemptedFingerprintRef.current = `${snapshot.draftPath}:${snapshot.fingerprint}`;
    pendingSavesRef.current += 1;
    try {
      const result = await requestCoordinator.queueComposeLatest(queueKey, async () => {
        const activeDraft = draftRef.current;
        const sameSession =
          activeDraft != null &&
          composeQueueKey(activeDraft.draftPath, activeDraft.recoverySourcePath) === queueKey &&
          (snapshot.draftId == null || activeDraft.draftId === snapshot.draftId);
        const recoveryTarget =
          recoveryTargetRef.current?.sourcePath === sourcePath &&
          recoveryTargetRef.current.draftId === snapshot.draftId
            ? recoveryTargetRef.current
            : undefined;
        const submittedPath = sameSession
          ? activeDraft.draftPath
          : recoveryTarget
            ? recoveryTarget.draftPath
            : snapshot.draftPath;
        const expectedRevision = sameSession
          ? (activeDraft.revision ?? snapshot.revision)
          : recoveryTarget
            ? recoveryTarget.revision
            : snapshot.revision;
        const submittedAttachments = snapshot.frontmatter.attach.map(
          (path) => recoveryTarget?.attachmentPaths?.get(path) ?? path,
        );
        const response = await (async () => {
          try {
            return await updateSession.mutateAsync({
              accountId: snapshot.accountId,
              draftPath: submittedPath,
              frontmatter: { ...snapshot.frontmatter, attach: submittedAttachments },
              body: snapshot.body,
              expectedRevision,
            });
          } catch (error) {
            const accepted = acceptedDraftRevision(error);
            const recovery = recoveredComposeSession(error);
            const latest = draftRef.current;
            if (
              recovery &&
              recovery.attachment_paths.length === snapshot.frontmatter.attach.length &&
              recovery.draft_id === snapshot.draftId &&
              recovery.revision ===
                (accepted && accepted.draft_id === snapshot.draftId
                  ? accepted.revision
                  : expectedRevision) &&
              recovery.draft_path !== submittedPath
            ) {
              const attachmentPaths = new Map(recoveryTarget?.attachmentPaths);
              snapshot.frontmatter.attach.forEach((path, index) => {
                const preserved = recovery.attachment_paths[index];
                if (preserved != null) attachmentPaths.set(path, preserved);
              });
              recoveryTargetRef.current = {
                attachmentPaths,
                sourcePath,
                draftPath: recovery.draft_path,
                draftId: snapshot.draftId,
                revision: recovery.revision,
              };
              const recovered = {
                ...snapshot,
                frontmatter: remapAttachments(snapshot.frontmatter, attachmentPaths),
                draftPath: recovery.draft_path,
                recoverySourcePath: sourcePath,
                bodyMarkdown: snapshot.body,
                rawContent: "",
                kind: "recovery",
                issues: [],
                revision: recovery.revision,
              };
              const belongsToActive =
                latest != null &&
                composeQueueKey(latest.draftPath, latest.recoverySourcePath) === queueKey &&
                latest.draftId === snapshot.draftId;
              if (belongsToActive) {
                lastSavedFingerprintRef.current = null;
                draftRef.current = {
                  ...latest,
                  frontmatter: remapAttachments(latest.frontmatter, attachmentPaths),
                  draftPath: recovery.draft_path,
                  recoverySourcePath: sourcePath,
                  revision: recovery.revision,
                };
                setDraft((value) =>
                  value != null &&
                  composeQueueKey(value.draftPath, value.recoverySourcePath) === queueKey &&
                  value.draftId === snapshot.draftId
                    ? {
                        ...value,
                        frontmatter: remapAttachments(value.frontmatter, attachmentPaths),
                        draftPath: recovery.draft_path,
                        recoverySourcePath: sourcePath,
                        revision: recovery.revision,
                      }
                    : value,
                );
              }
              if (belongsToActive || currentIntentKeyRef.current !== intentKey)
                rememberActiveDraft(intentKey, recovered);
              if (belongsToActive)
                setSaveError(`${errorMessage(error)} Your edits are preserved locally.`);
              // The bridge confirmed this exact snapshot was preserved locally.
              // Its token is still the observed canonical token, never acceptance.
              const preserved: ComposeSessionResponse = {
                session: {
                  draftPath: recovery.draft_path,
                  draftId: recovery.draft_id,
                  revision: recovery.revision,
                  recoverySourcePath: sourcePath,
                  accountId: snapshot.accountId,
                  rawContent: "",
                  frontmatter: remapAttachments(snapshot.frontmatter, attachmentPaths),
                  bodyMarkdown: snapshot.body,
                  previewHtml: "",
                  issues: [],
                  kind: "recovery",
                },
              };
              return preserved;
            } else if (
              accepted &&
              accepted.draft_id === snapshot.draftId &&
              latest != null &&
              composeQueueKey(latest.draftPath, latest.recoverySourcePath) === queueKey &&
              latest.draftId === snapshot.draftId
            ) {
              draftRef.current = { ...latest, revision: accepted.revision };
              setDraft((value) =>
                value != null &&
                composeQueueKey(value.draftPath, value.recoverySourcePath) === queueKey &&
                value.draftId === snapshot.draftId
                  ? { ...value, revision: accepted.revision }
                  : value,
              );
            }
            throw error;
          }
        })();
        if (snapshot.draftId != null && response.session.draftId !== snapshot.draftId)
          throw new Error("The saved response belongs to another draft; your text stays here");
        const receivedAttachments = response.session.frontmatter.attach ?? [];
        if (receivedAttachments.length !== snapshot.frontmatter.attach.length)
          throw new Error("The saved response does not confirm this snapshot's attachments");
        const knownTarget = recoveryTargetRef.current;
        const attachmentPaths = new Map(
          knownTarget?.sourcePath === sourcePath && knownTarget.draftId === snapshot.draftId
            ? knownTarget.attachmentPaths
            : undefined,
        );
        snapshot.frontmatter.attach.forEach((path, index) => {
          const preserved = receivedAttachments[index];
          if (preserved != null) attachmentPaths.set(path, preserved);
        });
        recoveryTargetRef.current = {
          attachmentPaths,
          sourcePath,
          draftPath: response.session.draftPath,
          draftId: response.session.draftId ?? snapshot.draftId,
          revision: response.session.revision,
        };
        const latest = draftRef.current;
        if (
          latest != null &&
          composeQueueKey(latest.draftPath, latest.recoverySourcePath) === queueKey &&
          (snapshot.draftId == null || latest.draftId === snapshot.draftId)
        ) {
          draftRef.current = {
            ...latest,
            frontmatter: remapAttachments(latest.frontmatter, attachmentPaths),
            draftId: response.session.draftId ?? undefined,
            revision: response.session.revision,
          };
          setDraft((active) =>
            active != null &&
            composeQueueKey(active.draftPath, active.recoverySourcePath) === queueKey &&
            (snapshot.draftId == null || active.draftId === snapshot.draftId)
              ? {
                  ...active,
                  frontmatter: remapAttachments(active.frontmatter, attachmentPaths),
                  draftId: response.session.draftId ?? undefined,
                  revision: response.session.revision,
                }
              : active,
          );
        }
        return response;
      });
      if (result.status !== "committed") return undefined;
      const response = result.value;
      if (
        draftRef.current == null ||
        composeQueueKey(draftRef.current.draftPath, draftRef.current.recoverySourcePath) !==
          queueKey ||
        (snapshot.draftId != null && draftRef.current.draftId !== snapshot.draftId)
      )
        return undefined;
      const savedFingerprint = draftFingerprint({
        ...draftRef.current,
        accountId: snapshot.accountId,
        bodyMarkdown: snapshot.body,
        frontmatter: { ...snapshot.frontmatter, attach: response.session.frontmatter.attach ?? [] },
      });
      const recoverySource =
        response.session.recoverySourcePath ?? draftRef.current.recoverySourcePath;
      if (recoverySource)
        preservedFingerprintRef.current = `${response.session.draftPath}:${savedFingerprint}`;
      else lastSavedFingerprintRef.current = savedFingerprint;
      const latest = draftRef.current;
      if (latest && draftFingerprint(latest) === savedFingerprint) {
        const next = {
          ...draftFromSession(response.session, snapshot.accountId),
          recoverySourcePath: recoverySource,
          preservedRecoveryPath: latest.preservedRecoveryPath,
        };
        setDraft(next);
        if (recoverySource)
          preservedFingerprintRef.current = `${next.draftPath}:${draftFingerprint(next)}`;
        else lastSavedFingerprintRef.current = draftFingerprint(next);
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
      if (
        draftRef.current &&
        composeQueueKey(draftRef.current.draftPath, draftRef.current.recoverySourcePath) ===
          queueKey
      )
        setSaveError(message);
      throw error;
    } finally {
      pendingSavesRef.current -= 1;
    }
  }, [draftRef, intentKey, queryClient, setDirty, setDraft, updateSession]);

  // While the daemon is down nothing can save: say so and keep the text,
  // which stays dirty, so the debounce below runs again once it's back.
  const daemonDown = useDaemonDown();
  useEffect(() => {
    if (!daemonDown) {
      attemptedFingerprintRef.current = null;
      setSaveError((shown) => (shown === DAEMON_DOWN_NOTE ? null : shown));
    }
  }, [daemonDown]);

  useEffect(() => {
    if (!dirty || !draft) return;
    if (daemonDown) {
      setSaveError(DAEMON_DOWN_NOTE);
      return;
    }
    if (`${draft.draftPath}:${draftFingerprint(draft)}` === attemptedFingerprintRef.current) return;
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
    return (
      !current.recoverySourcePath && draftFingerprint(current) === lastSavedFingerprintRef.current
    );
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

function remapAttachments(
  frontmatter: ComposeFrontmatter,
  paths: ReadonlyMap<string, string>,
): ComposeFrontmatter {
  return { ...frontmatter, attach: frontmatter.attach.map((path) => paths.get(path) ?? path) };
}
