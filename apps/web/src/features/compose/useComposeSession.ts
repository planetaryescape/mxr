import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  useCallback,
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
  checkComposeSafety,
  createScheduledSend,
  discardComposeSession,
  fetchAccounts,
  refreshComposeSession,
  restoreComposeSession,
  saveComposeSession,
  saveLocalDraft,
  sendComposeSession,
  startComposeSession,
  updateComposeSession,
  uploadComposeAttachment,
  type ComposeFrontmatter,
  type ComposeIssue,
  type ComposeKind,
  type ComposeSession,
  type DraftAddress,
  type DraftSafetyReport,
  type RuntimeAccount,
} from "./api";
import { archiveMessages } from "@/features/mailbox/api";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { formatRelativeAge } from "@/lib/utils";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useUndo } from "@/state/undoStore";
import type {
  DraftLengthHint,
  DraftRefineKnobs,
  DraftSuggestionResponse,
  VoiceRegister,
} from "./types";

const activeDraftStorageKey = "mxr.compose.activeDrafts";

export interface ComposeDraftState {
  draftPath: string;
  rawContent: string;
  frontmatter: ComposeFrontmatter;
  bodyMarkdown: string;
  issues: ComposeIssue[];
  accountId: string;
  kind: string;
  editorCommand?: string;
  cursorLine?: number;
}

export interface ComposeIntent {
  key: string;
  title: string;
  kind: ComposeKind;
  messageId?: string;
  draftId?: string;
  prefillTo?: string;
  prefillSubject?: string;
}

interface ActiveDraftEntry {
  draftPath: string;
  accountId?: string;
  updatedAt: number;
}

interface ComposeSaveSnapshot {
  draftPath: string;
  accountId: string;
  fingerprint: string;
  frontmatter: ComposeFrontmatter;
  body: string;
}

interface Snippet {
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
  visibleIssues: ComposeIssue[];
  recipientCount: number;
  runtimeAccounts: RuntimeAccount[];
  selectedAccount: RuntimeAccount | undefined;
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
  confirmSend: () => Promise<void>;
  sendLaterOpen: boolean;
  setSendLaterOpen: Dispatch<SetStateAction<boolean>>;
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
  retrySave: () => void;
  addFiles: (files: FileList | File[]) => Promise<void>;
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

export interface ComposeSessionOptions {
  /** Called after a successful send instead of the default
   * navigate-to-Sent (surface hosts close in place). */
  onSent?: () => void;
  /** Called after a successful discard instead of navigating to inbox. */
  onDiscarded?: () => void;
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
  const lastSavedFingerprintRef = useRef<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [lastSavedAt, setLastSavedAt] = useState<Date | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [sendConfirmOpen, setSendConfirmOpen] = useState(false);
  const [sendLaterOpen, setSendLaterOpen] = useState(false);
  const [showCc, setShowCc] = useState(false);
  const [showBcc, setShowBcc] = useState(false);
  const [uploading, setUploading] = useState(0);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [pendingSends, setPendingSends] = useState(0);
  const [safetyReport, setSafetyReport] = useState<DraftSafetyReport | null>(null);
  const [safetyCheckError, setSafetyCheckError] = useState<string | null>(null);
  const [checkingSafety, setCheckingSafety] = useState(false);
  const hasAutofocusedRef = useRef(false);
  // Set per send pipeline run (cmd+shift+Enter); consumed at dispatch time so
  // the safety-dialog detour keeps the archive intent and a cancelled undo
  // window drops it.
  const archiveAfterSendRef = useRef(false);
  const [aiPurpose, setAiPurpose] = useState("");
  const [aiRegister, setAiRegister] = useState<VoiceRegister>("neutral");
  const [aiLength, setAiLength] = useState<DraftLengthHint>("medium");
  // Tone/length are inferred from the relationship by default; only send the
  // dials as an override once the user has adjusted them.
  const [aiOverridden, setAiOverridden] = useState(false);
  const [refineContext, setRefineContext] = useState("");
  const [draftSuggestion, setDraftSuggestion] = useState<DraftSuggestionResponse | null>(null);
  const [assistOpen, setAssistOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const toInputRef = useRef<HTMLInputElement>(null);
  const ccInputRef = useRef<HTMLInputElement>(null);
  const bccInputRef = useRef<HTMLInputElement>(null);

  const updateSession = useMutation({ mutationFn: updateComposeSession });
  const sendSession = useMutation({
    mutationFn: ({
      draftPath,
      accountId,
      overrideToken,
    }: {
      draftPath: string;
      accountId: string;
      overrideToken?: string;
    }) => sendComposeSession(draftPath, accountId, overrideToken),
  });
  const serverSave = useMutation({
    mutationFn: ({ draftPath, accountId }: { draftPath: string; accountId: string }) =>
      saveComposeSession(draftPath, accountId),
  });
  const discardSession = useMutation({ mutationFn: discardComposeSession });
  const scheduleSession = useMutation({
    mutationFn: async (at: Date) => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const now = new Date().toISOString();
      const draftId = crypto.randomUUID();
      await saveLocalDraft({
        id: draftId,
        account_id: current.accountId,
        intent: draftIntentFromKind(current.kind),
        to: parseDraftAddresses(current.frontmatter.to),
        cc: parseDraftAddresses(current.frontmatter.cc),
        bcc: parseDraftAddresses(current.frontmatter.bcc),
        subject: current.frontmatter.subject,
        body_markdown: current.bodyMarkdown,
        attachments: [...current.frontmatter.attach],
        created_at: now,
        updated_at: now,
      });
      await createScheduledSend(draftId, at);
    },
  });
  const draftForMe = useMutation({
    mutationFn: async () => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const email = firstAddress(current.frontmatter.to);
      if (!email) throw new Error("Add a recipient before drafting");
      const purpose = aiPurpose.trim() || current.frontmatter.subject.trim();
      if (!purpose) throw new Error("Describe what this email should do");
      return apiFetch<DraftSuggestionResponse>("/api/v1/mail/drafts/compose", {
        method: "POST",
        body: {
          account_id: current.accountId,
          to: { name: null, email },
          instruction: purpose,
          // Reply/forward: hand the daemon the source message so it drafts
          // against the whole conversation.
          ...(intent.messageId ? { source_message_id: intent.messageId } : {}),
          // Only override the inferred tone/length once the user adjusts it.
          ...(aiOverridden ? { register: aiRegister, length_hint: aiLength } : {}),
        },
      });
    },
    onSuccess: (suggestion) => applyDraftSuggestion(suggestion, "Draft inserted"),
    onError: (error) =>
      toast.error("Draft generation failed", { description: errorMessage(error) }),
  });
  const refineDraft = useMutation({
    mutationFn: async (knobs: DraftRefineKnobs) => {
      if (!intent.draftId) throw new Error("Refine is available for saved mxr drafts");
      return apiFetch<DraftSuggestionResponse>("/api/v1/mail/drafts/refine", {
        method: "POST",
        body: {
          draft_id: intent.draftId,
          knobs,
        },
      });
    },
    onSuccess: (suggestion) => applyDraftSuggestion(suggestion, "Draft refined"),
    onError: (error) => toast.error("Refine failed", { description: errorMessage(error) }),
  });

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
  }, [intent, sessionQuery.data?.session]);

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
        rememberActiveDraft(intent.key, next);
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
  }, [intent.key, queryClient, updateSession]);

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
  }, [saveCurrentDraft]);

  const hasUnsavedWork = dirty || updateSession.isPending || pendingSends > 0;
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

  const runtimeAccounts = accounts.data?.accounts ?? [];
  const selectedAccount = draft
    ? runtimeAccounts.find((account) => account.account_id === draft.accountId)
    : undefined;
  const saveStatus = updateSession.isPending
    ? "Saving..."
    : dirty
      ? "Unsaved changes"
      : lastSavedAt
        ? `Saved ${formatRelativeAge(lastSavedAt)} ago`
        : "Not saved yet";
  const visibleIssues = draft ? (dirty ? localComposeIssues(draft) : draft.issues) : [];
  const recipientCount = draft ? countRecipients(draft.frontmatter) : 0;
  const canServerSave = Boolean(selectedAccount?.capabilities?.supports_server_drafts);
  const busy =
    updateSession.isPending || sendSession.isPending || discardSession.isPending || uploading > 0;

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

  function isCurrentDraftSaved(current: ComposeDraftState): boolean {
    return draftFingerprint(current) === lastSavedFingerprintRef.current;
  }

  async function handleSaveClick() {
    await saveCurrentDraft();
    toast.success("Draft saved locally");
  }

  async function handleServerSaveClick() {
    await saveCurrentDraft();
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error("Draft changed while saving", { description: "Save again before server draft." });
      return;
    }
    const accountId = current.accountId;
    const draftPath = current.draftPath;
    await serverSave.mutateAsync({ draftPath, accountId });
    toast.success("Draft saved to server");
  }

  async function handleRefreshClick() {
    const current = draftRef.current;
    if (!current) return;
    const response = await refreshComposeSession(current.draftPath);
    const next = draftFromSession(response.session, current.accountId);
    setDraft(next);
    setDirty(false);
    setSaveError(null);
    setLastSavedAt(new Date());
    toast.success("Draft refreshed");
  }

  function handleAttachShortcut() {
    if (uploading > 0) return;
    fileInputRef.current?.click();
  }

  function handleComposeKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.defaultPrevented) return;
    if (!(event.metaKey || event.ctrlKey)) return;
    const key = event.key.toLowerCase();

    if (event.shiftKey && key === "c") {
      event.preventDefault();
      event.stopPropagation();
      revealCc();
      return;
    }
    if (event.shiftKey && key === "b") {
      event.preventDefault();
      event.stopPropagation();
      revealBcc();
      return;
    }
    if (event.shiftKey && key === "a") {
      event.preventDefault();
      event.stopPropagation();
      handleAttachShortcut();
      return;
    }
    if (event.shiftKey && key === "l") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy) requestSendLater();
      return;
    }
    if (event.shiftKey && key === "r") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy) void handleRefreshClick();
      return;
    }
    if (event.shiftKey && key === "s") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy && canServerSave) void handleServerSaveClick();
      return;
    }
    if (!event.shiftKey && key === "s") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy) void handleSaveClick();
      return;
    }
    if (event.shiftKey && event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy) requestSendAndArchive();
      return;
    }
    if (!event.shiftKey && event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy) requestSend();
      return;
    }
    if (!event.shiftKey && event.key === "Backspace") {
      event.preventDefault();
      event.stopPropagation();
      if (!busy) requestDiscard();
    }
  }

  function requestSend() {
    startSendPipeline(false);
  }

  /** cmd+shift+Enter: send, then archive the source conversation (replies
   * only — a new message has no source to archive). */
  function requestSendAndArchive() {
    startSendPipeline(true);
  }

  function startSendPipeline(archiveAfterSend: boolean) {
    const current = draftRef.current;
    if (!current) return;
    const errors = localComposeIssues(current).filter((issue) => issue.severity === "error");
    if (errors.length > 0) {
      toast.error("Fix compose errors before sending", { description: errors[0]?.message });
      return;
    }
    archiveAfterSendRef.current = archiveAfterSend;
    void runSendPipeline();
  }

  /** Save → safety check → clean drafts dispatch straight away; reports
   * with issues (or a failed check) open the confirm dialog instead. */
  async function runSendPipeline() {
    await saveCurrentDraft().catch((error: Error) => {
      toast.error("Save before send failed", { description: error.message });
    });
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error("Draft changed while saving", {
        description: "Retry send after the latest save.",
      });
      return;
    }
    setCheckingSafety(true);
    setSafetyCheckError(null);
    try {
      const { report } = await checkComposeSafety(current.draftPath, current.accountId);
      if (report.allowed && report.issues.length === 0) {
        setSafetyReport(null);
        dispatchSend();
        return;
      }
      setSafetyReport(report);
      setSendConfirmOpen(true);
    } catch (error) {
      // Fail closed into the dialog, not into a silent send.
      setSafetyReport(null);
      setSafetyCheckError(errorMessage(error));
      setSendConfirmOpen(true);
    } finally {
      setCheckingSafety(false);
    }
  }

  function requestSendLater() {
    const current = draftRef.current;
    if (!current) return;
    const errors = localComposeIssues(current).filter((issue) => issue.severity === "error");
    if (errors.length > 0) {
      toast.error("Fix compose errors before scheduling", { description: errors[0]?.message });
      return;
    }
    setSendLaterOpen(true);
  }

  /** Save → store as a local draft → schedule. Closes the composer like a
   * send; the daemon dispatches the stored draft at `at`. */
  async function scheduleSend(at: Date, label?: string) {
    await saveCurrentDraft().catch((error: Error) => {
      toast.error("Save before schedule failed", { description: error.message });
    });
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error("Draft changed while saving", {
        description: "Retry scheduling after the latest save.",
      });
      return;
    }
    try {
      await scheduleSession.mutateAsync(at);
    } catch (error) {
      toast.error("Schedule failed", { description: errorMessage(error) });
      return;
    }
    setSendLaterOpen(false);
    forgetActiveDraft(intent.key);
    toast.success("Send scheduled", {
      description: label ? `Sends ${label}` : undefined,
    });
    void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    if (options.onSent) {
      options.onSent();
    } else {
      await navigate({ to: "/m/$mailbox", params: { mailbox: "sent" } });
    }
  }

  function revealCc() {
    setShowCc(true);
    window.setTimeout(() => ccInputRef.current?.focus(), 0);
  }

  function revealBcc() {
    setShowBcc(true);
    window.setTimeout(() => bccInputRef.current?.focus(), 0);
  }

  /** Confirm from the safety dialog. Picks up the override token from the
   * report's blocking issue when one exists. */
  async function confirmSend() {
    const overrideToken =
      safetyReport && !safetyReport.allowed
        ? (safetyReport.issues.find((issue) => issue.override_token)?.override_token ?? undefined)
        : undefined;
    setSendConfirmOpen(false);
    setSafetyReport(null);
    setSafetyCheckError(null);
    dispatchSend(overrideToken);
  }

  /** Deferred dispatch with a configurable undo window. The pending window
   * counts as unsaved work so beforeunload warns — closing the tab here
   * would silently drop the send. Cancellable via the toast or global z. */
  function dispatchSend(overrideToken?: string) {
    const current = draftRef.current;
    if (!current) return;
    const accountId = current.accountId;
    const draftPath = current.draftPath;
    const windowSeconds = useUiPrefs.getState().undoSendSeconds;
    const archiveSourceId =
      archiveAfterSendRef.current && intent.messageId ? intent.messageId : undefined;
    archiveAfterSendRef.current = false;

    const fire = () => {
      sendSession
        .mutateAsync({ draftPath, accountId, overrideToken })
        .then(async () => {
          forgetActiveDraft(intent.key);
          toast.success("Message sent");
          if (archiveSourceId) {
            try {
              await archiveMessages([archiveSourceId]);
              void queryClient.invalidateQueries({ queryKey: ["mailbox"] });
              void queryClient.invalidateQueries({ queryKey: ["thread"] });
              toast.success("Conversation archived");
            } catch (error) {
              toast.error("Archive after send failed", { description: errorMessage(error) });
            }
          }
          if (options.onSent) {
            options.onSent();
          } else {
            await navigate({ to: "/m/$mailbox", params: { mailbox: "sent" } });
          }
        })
        .catch((err: Error) => toast.error("Send failed", { description: err.message }))
        .finally(() => setPendingSends((count) => Math.max(0, count - 1)));
    };

    setPendingSends((count) => count + 1);
    if (windowSeconds === 0) {
      fire();
      return;
    }

    let cancelled = false;
    const cancel = () => {
      if (cancelled) return;
      cancelled = true;
      window.clearTimeout(timer);
      useUndo.getState().setPendingSendCancel(null);
      setPendingSends((count) => Math.max(0, count - 1));
      toast.dismiss(toastId);
      toast.info("Send cancelled");
    };
    const toastId = toast(`Sending in ${windowSeconds}s`, {
      duration: windowSeconds * 1000,
      description: "z to cancel",
      action: { label: "Undo", onClick: cancel },
    });
    const timer = window.setTimeout(() => {
      if (cancelled) return;
      useUndo.getState().setPendingSendCancel(null);
      fire();
    }, windowSeconds * 1000);
    useUndo.getState().setPendingSendCancel(cancel);
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
    await discardSession.mutateAsync(current.draftPath);
    forgetActiveDraft(intent.key);
    setDiscardConfirmOpen(false);
    toast.success("Draft discarded");
    if (options.onDiscarded) {
      options.onDiscarded();
    } else {
      await navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
    }
  }

  function retrySave() {
    void saveCurrentDraft().catch((error: Error) => {
      toast.error("Save failed", { description: error.message });
    });
  }

  async function addFiles(files: FileList | File[]) {
    const current = draftRef.current;
    if (!current) return;
    const fileList = Array.from(files);
    if (fileList.length === 0) return;
    setUploading((value) => value + fileList.length);
    try {
      const paths = await Promise.all(
        fileList.map(async (file) => {
          const contentBase64 = await fileToBase64(file);
          const uploaded = await uploadComposeAttachment({
            draftPath: current.draftPath,
            filename: file.name,
            contentBase64,
          });
          return uploaded.path;
        }),
      );
      setDraft((latest) =>
        latest
          ? {
              ...latest,
              frontmatter: {
                ...latest.frontmatter,
                attach: [...latest.frontmatter.attach, ...paths],
              },
            }
          : latest,
      );
      setDirty(true);
      toast.success(`Attached ${fileList.length} ${fileList.length === 1 ? "file" : "files"}`);
    } catch (error) {
      toast.error("Attachment failed", { description: errorMessage(error) });
    } finally {
      setUploading((value) => Math.max(0, value - fileList.length));
    }
  }

  function removeAttachment(path: string) {
    const current = draftRef.current;
    if (!current) return;
    updateFrontmatter(
      "attach",
      current.frontmatter.attach.filter((item) => item !== path),
    );
  }

  function applyDraftSuggestion(suggestion: DraftSuggestionResponse, title: string) {
    setDraft((current) => (current ? { ...current, bodyMarkdown: suggestion.body } : current));
    setDraftSuggestion(suggestion);
    // Reflect the inferred tone in the dials (unless the user overrode it),
    // so opening "Adjust" shows what was actually used.
    if (!aiOverridden) {
      if (suggestion.inferred_register) setAiRegister(suggestion.inferred_register);
      if (suggestion.inferred_length) setAiLength(suggestion.inferred_length);
    }
    setDirty(true);
    toast.success(title, { description: `Generated by ${suggestion.model}` });
  }

  function runRefine(knobs: DraftRefineKnobs) {
    const addContext = refineContext.trim();
    refineDraft.mutate({
      ...knobs,
      ...(addContext ? { add_context: addContext } : {}),
    });
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
    recipientCount,
    runtimeAccounts,
    selectedAccount,
    canServerSave,
    busy,
    uploading,
    sending: sendSession.isPending || updateSession.isPending,
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

    sendConfirmOpen,
    setSendConfirmOpen,
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
    requestSend,
    confirmSend,
    sendLaterOpen,
    setSendLaterOpen,
    requestSendLater,
    scheduleSend,
    scheduling: scheduleSession.isPending,
    safetyReport,
    safetyCheckError,
    checkingSafety,
    requestDiscard,
    discardDraft,
    retrySave,
    addFiles,
    removeAttachment,

    assistOpen,
    setAssistOpen,
    aiPurpose,
    setAiPurpose,
    aiRegister,
    onRegisterChange: (value) => {
      setAiRegister(value);
      setAiOverridden(true);
    },
    aiLength,
    onLengthChange: (value) => {
      setAiLength(value);
      setAiOverridden(true);
    },
    aiOverridden,
    resetTone: () => setAiOverridden(false),
    refineContext,
    setRefineContext,
    draftSuggestion,
    generateDraft: () => draftForMe.mutate(),
    generating: draftForMe.isPending,
    runRefine,
    refining: refineDraft.isPending,
    canRefine: Boolean(intent.draftId),
  };
}

async function loadInitialComposeSession(intent: ComposeIntent) {
  if (intent.draftId) return restoreComposeSession(intent.draftId);
  const active = readActiveDraft(intent.key);
  if (active?.draftPath) {
    try {
      return await refreshComposeSession(active.draftPath);
    } catch {
      forgetActiveDraft(intent.key);
    }
  }
  return startComposeSession(intent.kind, intent.messageId);
}

function applyPrefill(
  draft: ComposeDraftState,
  intent: ComposeIntent,
): { draft: ComposeDraftState; changed: boolean } {
  if (intent.kind !== "new") return { draft, changed: false };
  const to = intent.prefillTo?.trim();
  const subject = intent.prefillSubject?.trim();
  let changed = false;
  const frontmatter = { ...draft.frontmatter };
  if (to && !frontmatter.to.trim()) {
    frontmatter.to = to;
    changed = true;
  }
  if (subject && !frontmatter.subject.trim()) {
    frontmatter.subject = subject;
    changed = true;
  }
  return changed ? { draft: { ...draft, frontmatter }, changed } : { draft, changed };
}

function draftFromSession(session: ComposeSession, fallbackAccountId = ""): ComposeDraftState {
  return {
    draftPath: session.draftPath,
    rawContent: session.rawContent,
    frontmatter: {
      to: session.frontmatter.to ?? "",
      cc: session.frontmatter.cc ?? "",
      bcc: session.frontmatter.bcc ?? "",
      subject: session.frontmatter.subject ?? "",
      from: session.frontmatter.from ?? "",
      attach: session.frontmatter.attach ?? [],
    },
    bodyMarkdown: session.bodyMarkdown ?? "",
    issues: session.issues ?? [],
    accountId: session.accountId ?? fallbackAccountId,
    kind: session.kind ?? "new",
    editorCommand: session.editorCommand,
    cursorLine: session.cursorLine,
  };
}

function captureSaveSnapshot(draft: ComposeDraftState): ComposeSaveSnapshot {
  return {
    draftPath: draft.draftPath,
    accountId: draft.accountId,
    fingerprint: draftFingerprint(draft),
    frontmatter: { ...draft.frontmatter, attach: [...draft.frontmatter.attach] },
    body: draft.bodyMarkdown,
  };
}

function composeQueueKey(draftPath: string): string {
  return `compose:${draftPath}`;
}

function draftFingerprint(draft: ComposeDraftState): string {
  return JSON.stringify({
    to: draft.frontmatter.to,
    cc: draft.frontmatter.cc,
    bcc: draft.frontmatter.bcc,
    subject: draft.frontmatter.subject,
    from: draft.frontmatter.from,
    attach: draft.frontmatter.attach,
    body: draft.bodyMarkdown,
  });
}

function localComposeIssues(draft: ComposeDraftState): ComposeIssue[] {
  const issues: ComposeIssue[] = [];
  if (!draft.frontmatter.to.trim())
    issues.push({ severity: "error", message: "No recipients (to: field is empty)" });
  for (const address of splitAddresses(
    `${draft.frontmatter.to},${draft.frontmatter.cc},${draft.frontmatter.bcc}`,
  )) {
    if (!address.includes("@"))
      issues.push({ severity: "error", message: `Invalid email address: ${address}` });
  }
  if (!draft.frontmatter.subject.trim())
    issues.push({ severity: "warning", message: "Subject is empty" });
  if (!draft.bodyMarkdown.trim())
    issues.push({ severity: "warning", message: "Message body is empty" });
  return issues;
}

function splitAddresses(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function firstAddress(value: string): string | undefined {
  const first = splitAddresses(value)[0];
  if (!first) return undefined;
  const match = first.match(/<([^>]+)>/);
  return (match?.[1] ?? first).trim() || undefined;
}

function draftIntentFromKind(kind: string): ComposeKind {
  return kind === "reply" || kind === "reply_all" || kind === "forward" ? kind : "new";
}

/** "Name <email>" / bare-email chips → daemon `Address` values. */
function parseDraftAddresses(value: string): DraftAddress[] {
  return splitAddresses(value).map((raw) => {
    const match = raw.match(/^(.*?)\s*<([^>]+)>$/);
    if (match?.[2]) return { name: match[1]?.trim() || null, email: match[2].trim() };
    return { name: null, email: raw };
  });
}

function countRecipients(frontmatter: ComposeFrontmatter): number {
  return splitAddresses(`${frontmatter.to},${frontmatter.cc},${frontmatter.bcc}`).length;
}

function readActiveDraft(key: string): ActiveDraftEntry | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Record<string, ActiveDraftEntry>;
    return parsed[key];
  } catch {
    return undefined;
  }
}

function rememberActiveDraft(key: string, draft: ComposeDraftState) {
  if (typeof window === "undefined") return;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    const parsed = raw ? (JSON.parse(raw) as Record<string, ActiveDraftEntry>) : {};
    parsed[key] = { draftPath: draft.draftPath, accountId: draft.accountId, updatedAt: Date.now() };
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
  } catch {
    // Reload survival is best-effort only.
  }
}

function forgetActiveDraft(key: string) {
  if (typeof window === "undefined") return;
  try {
    const raw = window.localStorage.getItem(activeDraftStorageKey);
    if (!raw) return;
    const parsed = JSON.parse(raw) as Record<string, ActiveDraftEntry>;
    delete parsed[key];
    window.localStorage.setItem(activeDraftStorageKey, JSON.stringify(parsed));
  } catch {
    window.localStorage.removeItem(activeDraftStorageKey);
  }
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener(
      "load",
      () => {
        const value = String(reader.result ?? "");
        resolve(value.includes(",") ? value.slice(value.indexOf(",") + 1) : value);
      },
      { once: true },
    );
    reader.addEventListener(
      "error",
      () => reject(reader.error ?? new Error("Failed to read file")),
      { once: true },
    );
    reader.readAsDataURL(file);
  });
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function expandSnippet(value: string, snippets: Snippet[]): string {
  const match = value.match(/(^|\s);([A-Za-z0-9_-]+) $/);
  if (!match) return value;
  const snippet = snippets.find((item) => item.name === match[2]);
  if (!snippet) return value;
  return `${value.slice(0, match.index)}${match[1] ?? ""}${snippet.body}`;
}
