/*
 * Compose session lifecycle: the single hook every compose surface uses.
 * The heavy lifting lives in ./session/ (autosave, send pipeline,
 * attachments, draft assist, shortcuts); this file wires them together
 * and owns the draft buffer itself.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import { requestCoordinator } from "@/lib/requestCoordinator";
import {
  discardComposeSession,
  setComposeAudience,
  type ComposeFrontmatter,
  type ComposeSessionResponse,
} from "./api";
import { formatRelativeAge } from "@/lib/utils";
import {
  forgetActiveDraft,
  loadInitialComposeSession,
  UnownedPrivateDraftError,
  rememberActiveDraft,
} from "./session/activeDrafts";
import {
  applyPrefill,
  countRecipients,
  draftFingerprint,
  composeQueueKey,
  draftFromSession,
  errorMessage,
  expandSnippet,
  localComposeIssues,
  isMalformedAddressIssue,
  splitAddresses,
  type ComposeDraftState,
  type ComposeIntent,
  type Snippet,
} from "./session/composeDraft";
import type { ComposeController, Signature } from "./session/composeController";
import { handleComposeShortcut } from "./session/composeShortcuts";
import { useCollaboratorSuggestions } from "./session/useCollaboratorSuggestions";
import { useComposeAttachments } from "./session/useComposeAttachments";
import { useComposeAutofocus } from "./session/useComposeAutofocus";
import { useComposeAutosave } from "./session/useComposeAutosave";
import { useComposeSend, type ComposeSessionOptions } from "./session/useComposeSend";
import { useDraftAssist } from "./session/useDraftAssist";
import { mapModelSpans, NO_SPANS } from "./assist/modelSpans";
import { useDraftSaveActions } from "./session/useDraftSaveActions";
import { useSenderAccounts } from "./session/useSenderAccounts";

export type { ComposeDraftState, ComposeIntent, Snippet } from "./session/composeDraft";
export type { ComposeController, Signature } from "./session/composeController";
export type { ComposeUploadProgress } from "./session/useComposeAttachments";
export type { ComposeSessionOptions } from "./session/useComposeSend";

function audienceIdentity(value: ComposeDraftState) {
  return JSON.stringify({
    draftPath: value.draftPath,
    draftId: value.draftId,
    accountId: value.accountId,
    revision: value.revision,
    to: value.frontmatter.to.trim(),
    cc: value.frontmatter.cc.trim(),
    bcc: value.frontmatter.bcc.trim(),
  });
}

export function useComposeSession(
  intent: ComposeIntent,
  options: ComposeSessionOptions = {},
): ComposeController {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const snippets = useQuery({
    queryKey: ["snippets"],
    queryFn: () => apiFetch<{ snippets: Snippet[] }>("/api/v1/mail/snippets"),
    staleTime: 60_000,
  });
  const canonicalReopenRef = useRef(false);
  const audienceOperationRef = useRef(0);
  const audienceOperationLockedRef = useRef(false);
  const sessionQuery = useQuery({
    queryKey: ["compose-session", intent.key],
    queryFn: () => loadInitialComposeSession(intent, canonicalReopenRef.current),
    retry: false,
    staleTime: Infinity,
    // Reopening an intent (undoing a send, a reply closed and reopened)
    // must read the saved file, not the session as it was first loaded.
    gcTime: 0,
    // Loading it again resets the editor to the saved file, dropping text
    // typed since: never as part of a refresh after an outage.
    meta: { keepThroughGaps: true },
  });

  const [draft, setDraft] = useState<ComposeDraftState | null>(null);
  const draftRef = useRef<ComposeDraftState | null>(null);
  draftRef.current = draft;
  const [dirty, setDirty] = useState(false);
  const [snippetPickerOpen, setSnippetPickerOpen] = useState(false);
  const [signaturePickerOpen, setSignaturePickerOpen] = useState(false);
  const [signatureFoldOpen, setSignatureFoldOpen] = useState(false);
  // Fetched lazily when the signature fold or picker opens.
  const signatures = useQuery({
    queryKey: ["signatures"],
    queryFn: () => apiFetch<{ signatures: Signature[] }>("/api/v1/mail/signatures"),
    enabled: signaturePickerOpen || signatureFoldOpen,
    staleTime: 60_000,
  });
  const [showCc, setShowCc] = useState(false);
  const [showBcc, setShowBcc] = useState(false);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [changingAudience, setChangingAudience] = useState(false);
  const [audiencePreview, setAudiencePreview] = useState<ComposeSessionResponse | null>(null);
  const audiencePreviewFingerprintRef = useRef<string | null>(null);
  // Validation stays quiet until it can help: after the first send attempt,
  // or (for malformed addresses only) once a recipient field is left.
  const [sendAttempted, setSendAttempted] = useState(false);
  const [recipientsTouched, setRecipientsTouched] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const toInputRef = useRef<HTMLInputElement>(null);
  const ccInputRef = useRef<HTMLInputElement>(null);
  const bccInputRef = useRef<HTMLInputElement>(null);

  const autosave = useComposeAutosave({
    intentKey: intent.key,
    queryClient,
    draft,
    draftRef,
    operationLockRef: audienceOperationLockedRef,
    operationPaused: changingAudience,
    setDraft,
    dirty,
    setDirty,
  });
  const {
    saveCurrentDraft,
    isCurrentDraftSaved,
    lastSavedFingerprintRef,
    lastSavedAt,
    setLastSavedAt,
    saveError,
    setSaveError,
  } = autosave;
  const send = useComposeSend({
    intent,
    options,
    navigate,
    queryClient,
    draftRef,
    setDraft,
    saveCurrentDraft,
    isCurrentDraftSaved,
    markSessionFinished: autosave.markSessionFinished,
    markSendAttempted: () => setSendAttempted(true),
    onValidationBlocked: () => {
      const current = draftRef.current;
      if (current && !current.frontmatter.to.trim()) toInputRef.current?.focus();
    },
  });
  const attachments = useComposeAttachments({ draftRef, setDraft, setDirty });
  const assist = useDraftAssist({ intent, draftRef, setDraft, setDirty });

  const { handleSaveClick, handleServerSaveClick, handleRefreshClick, handleRecoveryClick } =
    useDraftSaveActions({
      intent,
      draftRef,
      setDraft,
      setDirty,
      saveCurrentDraft,
      isCurrentDraftSaved,
      lastSavedFingerprintRef,
      setSaveError,
      setLastSavedAt,
    });
  const discardSession = useMutation({
    mutationFn: ({ draftPath, revision }: { draftPath: string; revision?: number | null }) =>
      discardComposeSession(draftPath, revision),
  });

  useEffect(() => {
    const session = sessionQuery.data?.session;
    if (!session) return;
    const baseDraft = draftFromSession(session);
    const { draft: loaded, changed } = applyPrefill(baseDraft, intent);
    const next = { ...loaded, preservedRecoveryPath: sessionQuery.data?.preservedRecoveryPath };
    setDraft(next);
    setDirty(changed || Boolean(sessionQuery.data?.retainedSnapshot));
    setSaveError(
      next.recoverySourcePath
        ? "These edits are preserved locally. Use Retry or Open saved draft to continue."
        : next.preservedRecoveryPath
          ? `Your rejected edits remain in ${next.preservedRecoveryPath}`
          : null,
    );
    // Loading a session is not a save: nothing is claimed until one lands.
    setLastSavedAt(null);
    lastSavedFingerprintRef.current =
      sessionQuery.data?.retainedSnapshot || next.recoverySourcePath
        ? null
        : changed
          ? draftFingerprint(baseDraft)
          : draftFingerprint(next);
    setShowCc(Boolean(next.frontmatter.cc.trim()));
    setShowBcc(Boolean(next.frontmatter.bcc.trim()));
    rememberActiveDraft(intent.key, next);
  }, [
    intent,
    sessionQuery.data?.session,
    sessionQuery.data?.retainedSnapshot,
    sessionQuery.data?.preservedRecoveryPath,
    lastSavedFingerprintRef,
    setLastSavedAt,
    setSaveError,
  ]);

  useEffect(() => {
    if (draft && !rememberActiveDraft(intent.key, draft))
      setSaveError(
        "Browser storage cannot retain private edits across reload. Keep this editor open until they are saved or copied.",
      );
  }, [draft, intent.key, setSaveError]);

  const hasUnsavedWork = dirty || autosave.saving || send.pendingSends > 0;
  useEffect(() => {
    if (!hasUnsavedWork) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [hasUnsavedWork]);

  useComposeAutofocus(draft?.draftPath, draftRef, toInputRef);

  const collaboratorSuggestions = useCollaboratorSuggestions(draft);
  const { runtimeAccounts, selectedAccount, accountAddresses } = useSenderAccounts(draft);
  const saveStatus = saveStatusLabel({
    saving: autosave.saving,
    dirty,
    lastSavedAt,
    recovery: Boolean(draft?.recoverySourcePath),
  });
  const visibleIssues = !draft
    ? []
    : sendAttempted
      ? dirty
        ? localComposeIssues(draft)
        : draft.issues
      : recipientsTouched
        ? localComposeIssues(draft).filter(isMalformedAddressIssue)
        : [];
  const recipientCount = draft ? countRecipients(draft.frontmatter) : 0;
  const canServerSave =
    !draft?.recoverySourcePath && Boolean(selectedAccount?.capabilities?.supports_server_drafts);
  // A send in flight (including its undo window) counts as busy so Send,
  // Send later and the shortcuts cannot start a second one.
  const busy =
    autosave.saving ||
    changingAudience ||
    send.sendLocked ||
    send.sendPending ||
    discardSession.isPending ||
    attachments.uploading > 0;

  useEffect(() => {
    audienceOperationRef.current += 1;
    audienceOperationLockedRef.current = false;
    setChangingAudience(false);
    setAudiencePreview(null);
    audiencePreviewFingerprintRef.current = null;
    return () => {
      audienceOperationRef.current += 1;
      audienceOperationLockedRef.current = false;
    };
  }, [intent.key]);

  function beginAudienceOperation() {
    if (audienceOperationLockedRef.current) return null;
    audienceOperationLockedRef.current = true;
    const token = ++audienceOperationRef.current;
    setChangingAudience(true);
    return token;
  }

  function finishAudienceOperation(token: number) {
    if (audienceOperationRef.current !== token) return;
    audienceOperationLockedRef.current = false;
    setChangingAudience(false);
  }

  async function requestReplyAudience(replyAll: boolean) {
    const token = beginAudienceOperation();
    if (token == null) return;
    const current = draftRef.current;
    if (!current?.draftId || current.revision == null || current.recoverySourcePath) {
      finishAudienceOperation(token);
      toast.error("Reply audience is not ready", {
        description: "Save or reopen the canonical reply before changing its audience.",
      });
      return;
    }
    try {
      if (dirty) await saveCurrentDraft();
      await requestCoordinator.settleCompose(
        composeQueueKey(current.draftPath, current.recoverySourcePath),
      );
      const saved = draftRef.current;
      if (!saved?.draftId || saved.revision == null || saved.recoverySourcePath) return;
      const preview = await setComposeAudience({
        draftPath: saved.draftPath,
        draftId: saved.draftId,
        replyAll,
        expectedRevision: saved.revision,
        dryRun: true,
      });
      if (
        audienceOperationRef.current === token &&
        draftRef.current?.draftPath === saved.draftPath &&
        draftRef.current.draftId === saved.draftId
      ) {
        setAudiencePreview(preview);
        audiencePreviewFingerprintRef.current = audienceIdentity(saved);
      }
    } catch (error) {
      if (audienceOperationRef.current === token)
        toast.error("Could not preview the reply audience", { description: errorMessage(error) });
    } finally {
      finishAudienceOperation(token);
    }
  }

  async function confirmReplyAudience() {
    const requested = audiencePreview?.session.kind === "reply_all";
    const current = draftRef.current;
    if (!audiencePreview || requested == null || !current?.draftId || current.revision == null)
      return;
    const displayedFingerprint = audiencePreviewFingerprintRef.current;
    const token = beginAudienceOperation();
    if (token == null) return;
    try {
      if (displayedFingerprint !== audienceIdentity(current)) {
        const fresh = await setComposeAudience({
          draftPath: current.draftPath,
          draftId: current.draftId,
          replyAll: requested,
          expectedRevision: current.revision,
          dryRun: true,
        });
        if (audienceOperationRef.current === token) {
          setAudiencePreview(fresh);
          audiencePreviewFingerprintRef.current = audienceIdentity(current);
        }
        return;
      }
      if (dirty) await saveCurrentDraft();
      await requestCoordinator.settleCompose(
        composeQueueKey(current.draftPath, current.recoverySourcePath),
      );
      const saved = draftRef.current;
      if (!saved?.draftId || saved.revision == null || saved.recoverySourcePath) return;
      const preview = await setComposeAudience({
        draftPath: saved.draftPath,
        draftId: saved.draftId,
        replyAll: requested,
        expectedRevision: saved.revision,
        dryRun: true,
      });
      const displayedAudience = audienceFingerprint(audiencePreview.session.frontmatter);
      const currentAudience = audienceFingerprint(preview.session.frontmatter);
      if (
        displayedFingerprint !== audienceIdentity(saved) ||
        displayedAudience !== currentAudience
      ) {
        if (audienceOperationRef.current === token) {
          setAudiencePreview(preview);
          audiencePreviewFingerprintRef.current = audienceIdentity(saved);
        }
        return;
      }
      const applied = await setComposeAudience({
        draftPath: saved.draftPath,
        draftId: saved.draftId,
        replyAll: requested,
        expectedRevision: preview.session.revision ?? saved.revision,
        dryRun: false,
      });
      const latest = draftRef.current;
      if (
        audienceOperationRef.current !== token ||
        !latest ||
        latest.draftPath !== saved.draftPath ||
        latest.draftId !== saved.draftId
      )
        return;
      const next = { ...draftFromSession(applied.session), modelSpans: latest.modelSpans };
      draftRef.current = next;
      setDraft(next);
      setDirty(false);
      lastSavedFingerprintRef.current = draftFingerprint(next);
      setAudiencePreview(null);
      audiencePreviewFingerprintRef.current = null;
    } catch (error) {
      if (audienceOperationRef.current === token)
        toast.error("Reply audience was not changed", {
          description: `${errorMessage(error)} Your draft text remains available here.`,
        });
    } finally {
      finishAudienceOperation(token);
    }
  }

  function updateFrontmatter<K extends keyof ComposeFrontmatter>(
    field: K,
    value: ComposeFrontmatter[K],
  ) {
    if (audienceOperationLockedRef.current) return;
    setDraft((current) =>
      current ? { ...current, frontmatter: { ...current.frontmatter, [field]: value } } : current,
    );
    setDirty(true);
  }

  function updateBody(value: string) {
    if (audienceOperationLockedRef.current) return;
    const expanded = expandSnippet(value, snippets.data?.snippets ?? []);
    setDraft((current) =>
      current
        ? {
            ...current,
            bodyMarkdown: expanded,
            modelSpans: mapModelSpans(
              current.modelSpans ?? NO_SPANS,
              current.bodyMarkdown,
              expanded,
            ),
          }
        : current,
    );
    setDirty(true);
  }

  function updateAccount(accountId: string) {
    if (audienceOperationLockedRef.current) return;
    if (draftRef.current?.revision != null || autosave.saving) {
      toast.error("Saved drafts keep their account", {
        description: "Start a new draft to use another account.",
      });
      return;
    }
    const account = runtimeAccounts.find((item) => item.account_id === accountId);
    setDraft((current) =>
      current
        ? {
            ...current,
            accountId,
            frontmatter: {
              ...current.frontmatter,
              from: account?.email ?? current.frontmatter.from,
            },
          }
        : current,
    );
    setDirty(true);
  }

  function handleAttachShortcut() {
    if (audienceOperationLockedRef.current) return;
    if (attachments.uploading > 0) return;
    fileInputRef.current?.click();
  }

  function handleComposeKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    handleComposeShortcut(event, {
      busy,
      canServerSave,
      revealCc,
      revealBcc,
      handleAttachShortcut,
      requestSendLater: send.requestSendLater,
      openSignaturePicker: () => setSignaturePickerOpen(true),
      handleRefreshClick,
      handleServerSaveClick,
      openSnippetPicker: () => setSnippetPickerOpen(true),
      openDraftHelp: assist.openHelp,
      handleSaveClick,
      requestSendAndArchive: send.requestSendAndArchive,
      requestSend: send.requestSend,
      requestDiscard,
    });
  }

  function addCc(email: string) {
    if (audienceOperationLockedRef.current) return;
    const current = draftRef.current;
    if (!current) return;
    const everyone = splitAddresses(
      `${current.frontmatter.to},${current.frontmatter.cc},${current.frontmatter.bcc}`,
    );
    if (everyone.some((chip) => chip.toLowerCase().includes(email.toLowerCase()))) return;
    updateFrontmatter("cc", [...splitAddresses(current.frontmatter.cc), email].join(", "));
    setShowCc(true);
  }

  function insertSnippet(body: string) {
    if (audienceOperationLockedRef.current) return;
    const current = draftRef.current;
    if (!current) return;
    const base = current.bodyMarkdown;
    updateBody(base.trim() ? `${base.replace(/\n*$/, "")}\n\n${body}` : body);
    setSnippetPickerOpen(false);
  }

  function insertSignature(body: string) {
    if (audienceOperationLockedRef.current) return;
    const current = draftRef.current;
    if (!current) return;
    updateBody(`${current.bodyMarkdown.replace(/\n*$/, "")}\n\n--\n${body}`);
    setSignaturePickerOpen(false);
  }

  function revealCc() {
    setShowCc(true);
    window.setTimeout(() => ccInputRef.current?.focus(), 0);
  }

  function revealBcc() {
    setShowBcc(true);
    window.setTimeout(() => bccInputRef.current?.focus(), 0);
  }

  function requestDiscard() {
    if (dirty || draftRef.current?.revision != null || draftRef.current?.recoverySourcePath) {
      setDiscardConfirmOpen(true);
      return;
    }
    void discardDraft();
  }

  async function discardDraft() {
    const current = draftRef.current;
    if (!current) return;
    try {
      await discardSession.mutateAsync({
        draftPath: current.draftPath,
        revision: current.revision,
      });
    } catch (error) {
      toast.error("Discard failed", { description: errorMessage(error) });
      return;
    }
    autosave.markSessionFinished();
    forgetActiveDraft(intent.key, current.binding);
    setDiscardConfirmOpen(false);
    toast.success("Draft discarded");
    if (options.onDiscarded) {
      options.onDiscarded();
    } else {
      await navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
    }
  }

  /** Close the surface without losing the debounce window: save first, and
   * stay open with the error if the save fails. */
  async function requestClose() {
    try {
      await saveCurrentDraft();
    } catch (error) {
      toast.error("Draft not saved, composer kept open", {
        description: errorMessage(error),
      });
      return;
    }
    options.onClose?.();
  }

  function retrySave() {
    void handleSaveClick();
  }

  function removeAttachment(path: string) {
    if (audienceOperationLockedRef.current) return;
    const current = draftRef.current;
    if (!current) return;
    updateFrontmatter(
      "attach",
      current.frontmatter.attach.filter((item) => item !== path),
    );
  }

  return {
    intent,
    sessionLoading: sessionQuery.isLoading,
    sessionError: sessionQuery.isError ? sessionQuery.error : null,
    reopenSavedDraft:
      intent.draftId && sessionQuery.error instanceof UnownedPrivateDraftError
        ? () => {
            canonicalReopenRef.current = true;
            void sessionQuery.refetch();
          }
        : null,
    retrySession: () => {
      canonicalReopenRef.current = false;
      void sessionQuery.refetch();
    },

    draft,
    dirty,
    saveStatus,
    saveError,
    visibleIssues,
    markRecipientsTouched: () => setRecipientsTouched(true),
    recipientCount,
    runtimeAccounts,
    selectedAccount,
    accountAddresses,
    canServerSave,
    busy,
    uploading: attachments.uploading,
    sending: send.sendPending || autosave.saving,
    discarding: discardSession.isPending,
    changingAudience,
    audiencePreview,
    requestReplyAudience,
    confirmReplyAudience,
    cancelReplyAudience: () => {
      setAudiencePreview(null);
      audiencePreviewFingerprintRef.current = null;
    },

    showCc,
    setShowCc,
    showBcc,
    setShowBcc,
    revealCc,
    revealBcc,

    toInputRef,
    ccInputRef,
    bccInputRef,
    fileInputRef,

    sendConfirmOpen: send.sendConfirmOpen,
    setSendConfirmOpen: send.setSendConfirmOpen,
    discardConfirmOpen,
    setDiscardConfirmOpen,

    updateFrontmatter,
    updateBody,
    updateAccount,
    handleSaveClick,
    handleServerSaveClick,
    handleRefreshClick,
    handleRecoveryClick,
    handleAttachShortcut,
    handleComposeKeyDown,
    requestSend: send.requestSend,
    confirmSend: send.confirmSend,
    snippetPickerOpen,
    setSnippetPickerOpen,
    snippetList: snippets.data?.snippets ?? [],
    insertSnippet,
    signaturePickerOpen,
    setSignaturePickerOpen,
    signatureFoldOpen,
    setSignatureFoldOpen,
    signatureList: signatures.data?.signatures ?? [],
    insertSignature,
    collaboratorSuggestions,
    addCc,
    remindDialogOpen: send.remindDialogOpen,
    setRemindDialogOpen: send.setRemindDialogOpen,
    requestSendAndRemind: send.requestSendAndRemind,
    requestSendAndArchive: send.requestSendAndArchive,
    sendLaterOpen: send.sendLaterOpen,
    setSendLaterOpen: send.setSendLaterOpen,
    requestSendLater: send.requestSendLater,
    scheduleSend: send.scheduleSend,
    scheduling: send.scheduling,
    safetyReport: send.safetyReport,
    safetyCheckError: send.safetyCheckError,
    checkingSafety: send.checkingSafety,
    requestDiscard,
    discardDraft,
    requestClose,
    retrySave,
    addFiles: attachments.addFiles,
    uploadProgress: attachments.uploadProgress,
    removeAttachment,

    ...assist,
  };
}

/**
 * What the action bar says about saving. An untouched draft says nothing:
 * there is no work to be saved or lost yet.
 */
export function saveStatusLabel({
  saving,
  dirty,
  lastSavedAt,
  recovery = false,
}: {
  saving: boolean;
  dirty: boolean;
  lastSavedAt: Date | null;
  recovery?: boolean;
}): string {
  if (saving) return recovery ? "Preserving…" : "Saving…";
  if (dirty) return "Unsaved changes";
  if (recovery) return "Edits preserved locally";
  return lastSavedAt ? savedLabel(lastSavedAt) : "";
}

/** "Saved just now" for the first minute (never "Saved 0s ago"), then "Saved 4m ago". */
export function savedLabel(at: Date, now = Date.now()): string {
  if (now - at.getTime() < 60_000) return "Saved just now";
  return `Saved ${formatRelativeAge(at)} ago`;
}

function audienceFingerprint(
  frontmatter: Pick<Partial<ComposeFrontmatter>, "to" | "cc" | "bcc">,
): string {
  return JSON.stringify(
    [frontmatter.to, frontmatter.cc, frontmatter.bcc].map((value) =>
      (value ?? "")
        .split(/[;,]/)
        .map((address) => address.trim().toLowerCase())
        .filter(Boolean),
    ),
  );
}
