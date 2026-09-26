/*
 * Compose session lifecycle: the single hook every compose surface uses.
 * The heavy lifting lives in ./session/ (autosave, send pipeline,
 * attachments, draft assist, shortcuts); this file wires them together
 * and owns the draft buffer itself.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type KeyboardEvent,
  type RefObject,
  type SetStateAction,
} from "react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import {
  discardComposeSession,
  fetchAccounts,
  refreshComposeSession,
  saveComposeSession,
  suggestComposeCollaborators,
  type ComposeFrontmatter,
  type ComposeIssue,
  type DraftSafetyReport,
  type RuntimeAccount,
  type SuggestedCollaborator,
} from "./api";
import { fetchAccountAddresses } from "@/features/accounts/api";
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
import { handleComposeShortcut } from "./session/composeShortcuts";
import { useComposeAttachments, type ComposeUploadProgress } from "./session/useComposeAttachments";
import { useComposeAutosave } from "./session/useComposeAutosave";
import { useComposeSend, type ComposeSessionOptions } from "./session/useComposeSend";
import { useDraftAssist } from "./session/useDraftAssist";
import type {
  DraftLengthHint,
  DraftRefineKnobs,
  DraftSuggestionResponse,
  VoiceRegister,
} from "./types";

export type { ComposeDraftState, ComposeIntent, Snippet } from "./session/composeDraft";
export type { ComposeUploadProgress } from "./session/useComposeAttachments";
export type { ComposeSessionOptions } from "./session/useComposeSend";

export interface Signature {
  id: string;
  name: string;
  body: string;
}

/** Everything the compose UI consumes from the session lifecycle. */
export interface ComposeController {
  intent: ComposeIntent;
  sessionLoading: boolean;
  sessionError: Error | null;
  retrySession: () => void;

  draft: ComposeDraftState | null;
  dirty: boolean;
  saveStatus: string;
  saveError: string | null;
  /** Validation issues worth showing now: none before the first send
   * attempt, except malformed addresses once a recipient field is left. */
  visibleIssues: ComposeIssue[];
  markRecipientsTouched: () => void;
  recipientCount: number;
  runtimeAccounts: RuntimeAccount[];
  selectedAccount: RuntimeAccount | undefined;
  /** Send-as addresses (primary + aliases) for the selected account. */
  accountAddresses: string[];
  canServerSave: boolean;
  busy: boolean;
  uploading: number;
  sending: boolean;
  discarding: boolean;

  showCc: boolean;
  setShowCc: Dispatch<SetStateAction<boolean>>;
  showBcc: boolean;
  setShowBcc: Dispatch<SetStateAction<boolean>>;
  revealCc: () => void;
  revealBcc: () => void;

  toInputRef: RefObject<HTMLInputElement | null>;
  ccInputRef: RefObject<HTMLInputElement | null>;
  bccInputRef: RefObject<HTMLInputElement | null>;
  fileInputRef: RefObject<HTMLInputElement | null>;

  sendConfirmOpen: boolean;
  setSendConfirmOpen: Dispatch<SetStateAction<boolean>>;
  discardConfirmOpen: boolean;
  setDiscardConfirmOpen: Dispatch<SetStateAction<boolean>>;

  updateFrontmatter: <K extends keyof ComposeFrontmatter>(
    field: K,
    value: ComposeFrontmatter[K],
  ) => void;
  updateBody: (value: string) => void;
  updateAccount: (accountId: string) => void;
  handleSaveClick: () => Promise<void>;
  handleServerSaveClick: () => Promise<void>;
  handleRefreshClick: () => Promise<void>;
  handleAttachShortcut: () => void;
  handleComposeKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  requestSend: () => void;
  /** Send from the confirm dialog; `override` only after an explicit
   * override of a blocked safety report. */
  confirmSend: (override: boolean) => Promise<void>;
  snippetPickerOpen: boolean;
  setSnippetPickerOpen: Dispatch<SetStateAction<boolean>>;
  /** Snippets available for the picker and `;name ` inline expansion. */
  snippetList: Snippet[];
  /** Append a snippet body to the end of the message body. */
  insertSnippet: (body: string) => void;
  signaturePickerOpen: boolean;
  setSignaturePickerOpen: Dispatch<SetStateAction<boolean>>;
  signatureList: Signature[];
  /** Append a signature block (`\n\n--\n{body}`) to the message body. */
  insertSignature: (body: string) => void;
  /** "Maybe include" suggestions for the chip row; empty when none or the
   * lookup failed (the row hides silently). */
  collaboratorSuggestions: SuggestedCollaborator[];
  /** Append an address to Cc (suggested-collaborator click), revealing Cc. */
  addCc: (email: string) => void;
  sendLaterOpen: boolean;
  setSendLaterOpen: Dispatch<SetStateAction<boolean>>;
  /** Custom-time dialog for "Send and remind me if no reply in...". */
  remindDialogOpen: boolean;
  setRemindDialogOpen: Dispatch<SetStateAction<boolean>>;
  /** Send, then set a no-reply reminder for `at`. */
  requestSendAndRemind: (at: Date, label: string) => void;
  /** Send, then archive the source conversation (replies only). */
  requestSendAndArchive: () => void;
  /** Open the send-later dialog (same local validation gate as send). */
  requestSendLater: () => void;
  /** Persist the session as a stored draft and schedule it for `at`. */
  scheduleSend: (at: Date, label?: string) => Promise<void>;
  scheduling: boolean;
  /** Pre-send safety report backing the confirm dialog; null when the
   * check passed clean (no dialog) or hasn't run. */
  safetyReport: DraftSafetyReport | null;
  /** Set when the safety check itself failed — dialog shows a notice. */
  safetyCheckError: string | null;
  checkingSafety: boolean;
  requestDiscard: () => void;
  discardDraft: () => Promise<void>;
  /** Save pending edits, then call `options.onClose`. Stays open (with a
   * toast) when the save fails, so closing never drops text. */
  requestClose: () => Promise<void>;
  retrySave: () => void;
  addFiles: (files: FileList | File[]) => Promise<void>;
  /** In-flight upload entries for the attachments strip (cleared when the
   * batch settles). */
  uploadProgress: ComposeUploadProgress[];
  removeAttachment: (path: string) => void;

  assistOpen: boolean;
  setAssistOpen: Dispatch<SetStateAction<boolean>>;
  aiPurpose: string;
  setAiPurpose: Dispatch<SetStateAction<string>>;
  aiRegister: VoiceRegister;
  onRegisterChange: (value: VoiceRegister) => void;
  aiLength: DraftLengthHint;
  onLengthChange: (value: DraftLengthHint) => void;
  aiOverridden: boolean;
  resetTone: () => void;
  refineContext: string;
  setRefineContext: Dispatch<SetStateAction<string>>;
  draftSuggestion: DraftSuggestionResponse | null;
  generateDraft: () => void;
  generating: boolean;
  runRefine: (knobs: DraftRefineKnobs) => void;
  refining: boolean;
  canRefine: boolean;
}

export function useComposeSession(
  intent: ComposeIntent,
  options: ComposeSessionOptions = {},
): ComposeController {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts, staleTime: 60_000 });
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
  const hasAutofocusedRef = useRef(false);
  const [collaboratorSuggestions, setCollaboratorSuggestions] = useState<SuggestedCollaborator[]>(
    [],
  );
  // One collaborators lookup per draft path — recipients settling for 1s
  // with at least one To address triggers it.
  const collaboratorsFetchedRef = useRef(new Set<string>());
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

  const serverSave = useMutation({
    // Carry the local id so the daemon updates that row before making the
    // explicit provider copy. A new compose session has no local row to update.
    mutationFn: ({ draftPath, accountId }: { draftPath: string; accountId: string }) =>
      saveComposeSession(draftPath, accountId, intent.draftId),
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

  useEffect(() => {
    if (!draft?.draftPath || hasAutofocusedRef.current) return;
    hasAutofocusedRef.current = true;
    // Defer past the loading→loaded re-render, and never steal focus the
    // user has already placed somewhere else.
    requestAnimationFrame(() => {
      const active = document.activeElement;
      const focusIsElsewhere =
        active instanceof HTMLElement && active !== document.body && active.tabIndex >= 0;
      if (!focusIsElsewhere) toInputRef.current?.focus();
    });
  }, [draft?.draftPath]);

  // Suggest collaborators once per draft, after the recipients settle for a
  // second with at least one To address. Best-effort: errors hide the row.
  const collaboratorDraftPath = draft?.draftPath;
  const collaboratorAccountId = draft?.accountId;
  const collaboratorTo = draft?.frontmatter.to ?? "";
  useEffect(() => {
    if (!collaboratorDraftPath || !collaboratorAccountId) return;
    if (collaboratorsFetchedRef.current.has(collaboratorDraftPath)) return;
    if (splitAddresses(collaboratorTo).length === 0) return;
    const handle = window.setTimeout(() => {
      collaboratorsFetchedRef.current.add(collaboratorDraftPath);
      suggestComposeCollaborators(collaboratorDraftPath, collaboratorAccountId)
        .then((response) => setCollaboratorSuggestions(response.suggestions ?? []))
        .catch(() => {
          // Silently hide — suggestions are a nicety, never an error state.
        });
    }, 1000);
    return () => window.clearTimeout(handle);
  }, [collaboratorDraftPath, collaboratorAccountId, collaboratorTo]);

  const runtimeAccounts = accounts.data?.accounts ?? [];
  const selectedAccount = draft
    ? runtimeAccounts.find((account) => account.account_id === draft.accountId)
    : undefined;

  // Aliases the selected account may send as (send-as). Shares the cache key
  // used by the account-detail address editor so both stay consistent.
  const addressesQuery = useQuery({
    queryKey: ["account-addresses", selectedAccount?.account_id],
    queryFn: () => fetchAccountAddresses(selectedAccount?.account_id ?? ""),
    enabled: Boolean(selectedAccount?.account_id),
    staleTime: 60_000,
  });
  // Union of the account's primary email and its configured aliases, primary
  // first (it always leads because we prepend it), deduped, so the current
  // `from` always has a matching option in the picker.
  const accountAddresses: string[] = (() => {
    if (!selectedAccount) return [];
    const fetched = addressesQuery.data?.addresses ?? [];
    const emails = [selectedAccount.email, ...fetched.map((address) => address.email)].filter(
      (email) => email.length > 0,
    );
    return [...new Set(emails)];
  })();
  const saveStatus = autosave.saving
    ? "Saving..."
    : dirty
      ? "Unsaved changes"
      : lastSavedAt
        ? `Saved ${formatRelativeAge(lastSavedAt)} ago`
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

  // The three handlers below are fired from shortcuts, menus and editor ex
  // commands that never await them, so they report failures themselves and
  // never reject.
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
    collaboratorSuggestions: draft
      ? collaboratorSuggestions.filter(
          (item) =>
            !`${draft.frontmatter.to},${draft.frontmatter.cc},${draft.frontmatter.bcc}`
              .toLowerCase()
              .includes(item.email.toLowerCase()),
        )
      : [],
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
