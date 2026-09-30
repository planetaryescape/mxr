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
import { discardComposeSession, type ComposeFrontmatter } from "./api";
import { formatRelativeAge } from "@/lib/utils";
import {
  forgetActiveDraft,
  loadInitialComposeSession,
  rememberActiveDraft,
} from "./session/activeDrafts";
import {
  applyPrefill,
  countRecipients,
  draftFingerprint,
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
import { useDraftSaveActions } from "./session/useDraftSaveActions";
import { useSenderAccounts } from "./session/useSenderAccounts";

export type { ComposeDraftState, ComposeIntent, Snippet } from "./session/composeDraft";
export type { ComposeController, Signature } from "./session/composeController";
export type { ComposeUploadProgress } from "./session/useComposeAttachments";
export type { ComposeSessionOptions } from "./session/useComposeSend";

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
  const sessionQuery = useQuery({
    queryKey: ["compose-session", intent.key],
    queryFn: () => loadInitialComposeSession(intent),
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
  // Fetched lazily — the list is only needed once the picker opens.
  const signatures = useQuery({
    queryKey: ["signatures"],
    queryFn: () => apiFetch<{ signatures: Signature[] }>("/api/v1/mail/signatures"),
    enabled: signaturePickerOpen,
    staleTime: 60_000,
  });
  const [showCc, setShowCc] = useState(false);
  const [showBcc, setShowBcc] = useState(false);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
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

  const { handleSaveClick, handleServerSaveClick, handleRefreshClick } = useDraftSaveActions({
    intent,
    draftRef,
    setDraft,
    setDirty,
    saveCurrentDraft,
    isCurrentDraftSaved,
    setSaveError,
    setLastSavedAt,
  });
  const discardSession = useMutation({ mutationFn: discardComposeSession });

  useEffect(() => {
    const session = sessionQuery.data?.session;
    if (!session) return;
    const baseDraft = draftFromSession(session);
    const { draft: next, changed } = applyPrefill(baseDraft, intent);
    setDraft(next);
    setDirty(changed);
    setSaveError(null);
    setLastSavedAt(new Date());
    lastSavedFingerprintRef.current = changed
      ? draftFingerprint(baseDraft)
      : draftFingerprint(next);
    setShowCc(Boolean(next.frontmatter.cc.trim()));
    setShowBcc(Boolean(next.frontmatter.bcc.trim()));
    rememberActiveDraft(intent.key, next);
  }, [intent, sessionQuery.data?.session, lastSavedFingerprintRef, setLastSavedAt, setSaveError]);

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
  const saveStatus = autosave.saving
    ? "Saving…"
    : dirty
      ? "Unsaved changes"
      : lastSavedAt
        ? savedLabel(lastSavedAt)
        : "Not saved yet";
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
  const canServerSave = Boolean(selectedAccount?.capabilities?.supports_server_drafts);
  // A send in flight (including its undo window) counts as busy so Send,
  // Send later and the shortcuts cannot start a second one.
  const busy =
    autosave.saving ||
    send.sendLocked ||
    send.sendPending ||
    discardSession.isPending ||
    attachments.uploading > 0;

  function updateFrontmatter<K extends keyof ComposeFrontmatter>(
    field: K,
    value: ComposeFrontmatter[K],
  ) {
    setDraft((current) =>
      current ? { ...current, frontmatter: { ...current.frontmatter, [field]: value } } : current,
    );
    setDirty(true);
  }

  function updateBody(value: string) {
    const expanded = expandSnippet(value, snippets.data?.snippets ?? []);
    setDraft((current) => (current ? { ...current, bodyMarkdown: expanded } : current));
    setDirty(true);
  }

  function updateAccount(accountId: string) {
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
      handleSaveClick,
      requestSendAndArchive: send.requestSendAndArchive,
      requestSend: send.requestSend,
      requestDiscard,
    });
  }

  function addCc(email: string) {
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
    const current = draftRef.current;
    if (!current) return;
    const base = current.bodyMarkdown;
    updateBody(base.trim() ? `${base.replace(/\n*$/, "")}\n\n${body}` : body);
    setSnippetPickerOpen(false);
  }

  function insertSignature(body: string) {
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
    if (dirty) {
      setDiscardConfirmOpen(true);
      return;
    }
    void discardDraft();
  }

  async function discardDraft() {
    const current = draftRef.current;
    if (!current) return;
    try {
      await discardSession.mutateAsync(current.draftPath);
    } catch (error) {
      toast.error("Discard failed", { description: errorMessage(error) });
      return;
    }
    autosave.markSessionFinished();
    forgetActiveDraft(intent.key);
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
    void saveCurrentDraft().catch((error: Error) => {
      toast.error("Save failed", { description: error.message });
    });
  }

  function removeAttachment(path: string) {
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
    retrySession: () => {
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

/** "Saved just now" for the first minute (never "Saved 0s ago"), then "Saved 4m ago". */
export function savedLabel(at: Date, now = Date.now()): string {
  if (now - at.getTime() < 60_000) return "Saved just now";
  return `Saved ${formatRelativeAge(at)} ago`;
}
