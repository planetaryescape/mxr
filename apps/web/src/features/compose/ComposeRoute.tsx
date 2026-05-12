import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import {
  AlertTriangle,
  Check,
  FilePlus2,
  Loader2,
  Paperclip,
  RefreshCw,
  Send,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type DragEvent,
  type KeyboardEvent,
  type Ref,
} from "react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import {
  discardComposeSession,
  fetchAccounts,
  refreshComposeSession,
  restoreComposeSession,
  saveComposeSession,
  sendComposeSession,
  startComposeSession,
  updateComposeSession,
  uploadComposeAttachment,
  type ComposeFrontmatter,
  type ComposeIssue,
  type ComposeKind,
  type ComposeSession,
  type RuntimeAccount,
} from "./api";
import { EmptyState } from "@/components/EmptyState";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { cn, formatRelativeAge } from "@/lib/utils";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { useUiPrefs } from "@/state/uiPrefsStore";

const CodeMirrorComposeEditor = lazy(() =>
  import("./codemirror/CodeMirrorComposeEditor").then((module) => ({
    default: module.CodeMirrorComposeEditor,
  })),
);
const TiptapComposeEditor = lazy(() =>
  import("./tiptap/TiptapComposeEditor").then((module) => ({
    default: module.TiptapComposeEditor,
  })),
);

const activeDraftStorageKey = "mxr.compose.activeDrafts";

interface ComposeDraftState {
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

interface ComposeIntent {
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

type VoiceRegister = "casual" | "neutral" | "formal";
type DraftLengthHint = "short" | "medium" | "long";

interface VoiceMatchReport {
  score: number;
  confidence: string;
  notable_deltas: string[];
}

interface HumanizerReport {
  score: number;
  hits: { category: string; matched: string; suggestion?: string | null }[];
}

interface DraftSuggestionResponse {
  kind: "DraftSuggestion";
  body: string;
  model: string;
  voice_match?: VoiceMatchReport | null;
  humanizer?: HumanizerReport | null;
  rewrite_iterations: number;
}

interface DraftRefineKnobs {
  shorter?: boolean;
  warmer?: boolean;
  more_formal?: boolean;
  less_emoji?: boolean;
  add_context?: string;
}

type ComposeSearch = Record<string, unknown>;

export function ComposeRoute() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const location = useRouterState({ select: (state) => state.location });
  const intent = useMemo(
    () => composeIntent(location.pathname, location.search as ComposeSearch),
    [location.pathname, location.search],
  );
  const editorPreference = useUiPrefs((state) => state.composeEditor);
  const setComposeEditor = useUiPrefs((state) => state.setComposeEditor);

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
  const [showCc, setShowCc] = useState(false);
  const [showBcc, setShowBcc] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [uploading, setUploading] = useState(0);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [aiPurpose, setAiPurpose] = useState("");
  const [aiRegister, setAiRegister] = useState<VoiceRegister>("neutral");
  const [aiLength, setAiLength] = useState<DraftLengthHint>("medium");
  const [refineContext, setRefineContext] = useState("");
  const [draftSuggestion, setDraftSuggestion] = useState<DraftSuggestionResponse | null>(null);
  const dragDepth = useRef(0);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const toInputRef = useRef<HTMLInputElement>(null);
  const ccInputRef = useRef<HTMLInputElement>(null);
  const bccInputRef = useRef<HTMLInputElement>(null);

  const updateSession = useMutation({ mutationFn: updateComposeSession });
  const sendSession = useMutation({
    mutationFn: ({ draftPath, accountId }: { draftPath: string; accountId: string }) =>
      sendComposeSession(draftPath, accountId),
  });
  const serverSave = useMutation({
    mutationFn: ({ draftPath, accountId }: { draftPath: string; accountId: string }) =>
      saveComposeSession(draftPath, accountId),
  });
  const discardSession = useMutation({ mutationFn: discardComposeSession });
  const draftForMe = useMutation({
    mutationFn: async () => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const email = firstAddress(current.frontmatter.to);
      if (!email) throw new Error("Add a recipient before drafting");
      const purpose = aiPurpose.trim() || current.frontmatter.subject.trim();
      if (!purpose) throw new Error("Describe what this email should do");
      return apiFetch<DraftSuggestionResponse>("/api/v1/mail/drafts/new", {
        method: "POST",
        body: {
          account_id: current.accountId,
          to: { name: null, email },
          purpose,
          register: aiRegister,
          length_hint: aiLength,
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

  useEffect(() => {
    if (!draft?.draftPath) return;
    toInputRef.current?.focus();
  }, [draft?.draftPath]);

  if (sessionQuery.isLoading) {
    return <ComposeLoading title={intent.title} />;
  }

  if (sessionQuery.isError) {
    return (
      <EmptyState
        icon={RefreshCw}
        title="Compose unavailable"
        description={sessionQuery.error.message}
        action={<Button onClick={() => sessionQuery.refetch()}>Retry</Button>}
      />
    );
  }

  if (!draft) return null;

  const runtimeAccounts = accounts.data?.accounts ?? [];
  const selectedAccount = runtimeAccounts.find((account) => account.account_id === draft.accountId);
  const saveStatus = updateSession.isPending
    ? "Saving..."
    : dirty
      ? "Unsaved changes"
      : lastSavedAt
        ? `Saved ${formatRelativeAge(lastSavedAt)} ago`
        : "Not saved yet";
  const visibleIssues = dirty ? localComposeIssues(draft) : draft.issues;
  const recipientCount = countRecipients(draft.frontmatter);
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
    const current = draftRef.current;
    if (!current) return;
    const errors = localComposeIssues(current).filter((issue) => issue.severity === "error");
    if (errors.length > 0) {
      toast.error("Fix compose errors before sending", { description: errors[0]?.message });
      return;
    }
    setSendConfirmOpen(true);
  }

  function revealCc() {
    setShowCc(true);
    window.setTimeout(() => ccInputRef.current?.focus(), 0);
  }

  function revealBcc() {
    setShowBcc(true);
    window.setTimeout(() => bccInputRef.current?.focus(), 0);
  }

  async function confirmSend() {
    await saveCurrentDraft();
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error("Draft changed while saving", {
        description: "Retry send after the latest save.",
      });
      return;
    }
    const accountId = current.accountId;
    const draftPath = current.draftPath;
    await sendSession.mutateAsync({ draftPath, accountId });
    forgetActiveDraft(intent.key);
    setSendConfirmOpen(false);
    toast.success("Message sent");
    await navigate({ to: "/m/$mailbox", params: { mailbox: "sent" } });
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
    await navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
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

  function onDragEnter(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    event.preventDefault();
    dragDepth.current += 1;
    setDragActive(true);
  }

  function onDragOver(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    event.preventDefault();
  }

  function onDragLeave(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    dragDepth.current = Math.max(0, dragDepth.current - 1);
    if (dragDepth.current === 0) setDragActive(false);
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    event.preventDefault();
    dragDepth.current = 0;
    setDragActive(false);
    void addFiles(event.dataTransfer.files);
  }

  return (
    <div
      className="flex min-w-0 flex-1 overflow-auto bg-background"
      onKeyDown={handleComposeKeyDown}
    >
      <div className="mx-auto flex w-full max-w-[1080px] flex-col px-3 py-4 lg:px-4">
        <header className="mb-3 flex items-start justify-between gap-4 border-b border-border pb-3">
          <div className="min-w-0">
            <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
              Compose
            </div>
            <h1 className="truncate text-xl font-semibold tracking-tight">{intent.title}</h1>
            <div className="mt-1 text-xs text-muted-foreground">{saveStatus} · local draft</div>
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-2">
            <ComposeActionButton
              variant="ghost"
              onClick={handleRefreshClick}
              disabled={busy}
              shortcut="⇧⌘R"
            >
              <RefreshCw className="size-3" />
              Refresh
            </ComposeActionButton>
            <ComposeActionButton
              variant="secondary"
              onClick={handleSaveClick}
              disabled={busy}
              shortcut="⌘S"
            >
              {updateSession.isPending ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Check className="size-3" />
              )}
              Save
            </ComposeActionButton>
            {canServerSave ? (
              <ComposeActionButton
                variant="outline"
                onClick={handleServerSaveClick}
                disabled={busy || serverSave.isPending}
                shortcut="⇧⌘S"
              >
                Server draft
              </ComposeActionButton>
            ) : null}
            <ComposeActionButton
              variant="destructive"
              onClick={requestDiscard}
              disabled={busy}
              shortcut="⌘⌫"
            >
              <Trash2 className="size-3" />
              Discard
            </ComposeActionButton>
            <ComposeActionButton onClick={requestSend} disabled={busy} shortcut="⌘Enter">
              <Send className="size-3" />
              Send
            </ComposeActionButton>
          </div>
        </header>

        <div className="mb-3 rounded-lg border border-border bg-card px-3 py-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-[260px] flex-1">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <Sparkles className="size-4 text-primary" />
                Draft for me
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                Uses the local relationship profile when available. The draft stays editable.
              </p>
            </div>
            <DraftQualityBadges suggestion={draftSuggestion} />
          </div>
          <div className="mt-3 grid gap-3 lg:grid-cols-[1fr_140px_140px_auto] lg:items-end">
            <div>
              <Label htmlFor="compose-ai-purpose">Purpose</Label>
              <Textarea
                id="compose-ai-purpose"
                value={aiPurpose}
                onChange={(event) => setAiPurpose(event.target.value)}
                placeholder="Follow up on the deck and ask for feedback by Friday"
                className="mt-1 min-h-20 bg-background"
              />
            </div>
            <div>
              <Label>Register</Label>
              <Select
                value={aiRegister}
                onValueChange={(value) => setAiRegister(value as VoiceRegister)}
              >
                <SelectTrigger className="mt-1 h-9 bg-background">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="casual">Casual</SelectItem>
                  <SelectItem value="neutral">Neutral</SelectItem>
                  <SelectItem value="formal">Formal</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label>Length</Label>
              <Select
                value={aiLength}
                onValueChange={(value) => setAiLength(value as DraftLengthHint)}
              >
                <SelectTrigger className="mt-1 h-9 bg-background">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="short">Short</SelectItem>
                  <SelectItem value="medium">Medium</SelectItem>
                  <SelectItem value="long">Long</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Button
              type="button"
              onClick={() => draftForMe.mutate()}
              disabled={busy || draftForMe.isPending}
              className="gap-1.5"
            >
              {draftForMe.isPending ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Sparkles className="size-3" />
              )}
              Generate
            </Button>
          </div>
          <div className="mt-3 flex flex-wrap items-end gap-2 border-t border-border pt-3">
            <div className="min-w-[220px] flex-1">
              <Label htmlFor="compose-refine-context">Refine context</Label>
              <Input
                id="compose-refine-context"
                value={refineContext}
                onChange={(event) => setRefineContext(event.target.value)}
                placeholder="Optional extra context for refinement"
                className="mt-1 h-9 bg-background text-sm"
              />
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy || refineDraft.isPending || !intent.draftId}
              onClick={() => runRefine({ shorter: true })}
            >
              Shorter
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy || refineDraft.isPending || !intent.draftId}
              onClick={() => runRefine({ warmer: true })}
            >
              Warmer
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy || refineDraft.isPending || !intent.draftId}
              onClick={() => runRefine({ more_formal: true })}
            >
              More formal
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy || refineDraft.isPending || !intent.draftId}
              onClick={() => runRefine({ less_emoji: true })}
            >
              Less emoji
            </Button>
          </div>
          {!intent.draftId ? (
            <p className="mt-2 text-2xs text-muted-foreground">
              Refine buttons work on saved mxr drafts opened from the drafts list.
            </p>
          ) : null}
        </div>

        <div
          role="form"
          aria-label="Compose message"
          className="relative min-h-0 overflow-hidden rounded-lg border border-border bg-card"
          onDragEnter={onDragEnter}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
        >
          {dragActive ? (
            <div className="absolute inset-0 z-20 flex items-center justify-center rounded-2xl border border-primary bg-background/80 backdrop-blur-sm">
              <div className="rounded-xl border border-border bg-popover px-5 py-4 text-center shadow-xl">
                <FilePlus2 className="mx-auto mb-2 size-6 text-primary" />
                <div className="text-sm font-medium">Drop files to attach</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  mxr stores a local copy for this draft.
                </div>
              </div>
            </div>
          ) : null}

          <div className="space-y-2 border-b border-border px-3 py-3">
            <AddressField
              label="To"
              value={draft.frontmatter.to}
              inputRef={toInputRef}
              onChange={(value) => updateFrontmatter("to", value)}
            />
            <div className="flex gap-2 pl-12">
              {!showCc ? (
                <ComposeActionButton variant="ghost" size="sm" onClick={revealCc} shortcut="⇧⌘C">
                  Add Cc
                </ComposeActionButton>
              ) : null}
              {!showBcc ? (
                <ComposeActionButton variant="ghost" size="sm" onClick={revealBcc} shortcut="⇧⌘B">
                  Add Bcc
                </ComposeActionButton>
              ) : null}
            </div>
            <Collapsible open={showCc} onOpenChange={setShowCc}>
              <CollapsibleContent>
                <AddressField
                  label="Cc"
                  value={draft.frontmatter.cc}
                  inputRef={ccInputRef}
                  onChange={(value) => updateFrontmatter("cc", value)}
                />
              </CollapsibleContent>
            </Collapsible>
            <Collapsible open={showBcc} onOpenChange={setShowBcc}>
              <CollapsibleContent>
                <AddressField
                  label="Bcc"
                  value={draft.frontmatter.bcc}
                  inputRef={bccInputRef}
                  onChange={(value) => updateFrontmatter("bcc", value)}
                />
              </CollapsibleContent>
            </Collapsible>

            <div className="grid grid-cols-[42px_1fr] items-center gap-2">
              <Label htmlFor="compose-subject" className="text-muted-foreground">
                Subject
              </Label>
              <Input
                id="compose-subject"
                value={draft.frontmatter.subject}
                onChange={(event) => updateFrontmatter("subject", event.target.value)}
                placeholder="Subject"
                className="h-9 bg-background text-sm"
              />
            </div>

            <input
              ref={fileInputRef}
              type="file"
              multiple
              className="sr-only"
              onChange={(event) => {
                if (event.currentTarget.files) void addFiles(event.currentTarget.files);
                event.currentTarget.value = "";
              }}
            />
            <div className="mt-3 flex flex-wrap items-end justify-between gap-3 border-t border-border pt-3">
              <div className="flex min-w-0 flex-1 flex-wrap gap-3">
                <div className="min-w-[260px] flex-1">
                  <Label>Send from</Label>
                  <AccountSelect
                    accounts={runtimeAccounts}
                    value={draft.accountId}
                    onChange={updateAccount}
                  />
                </div>
                <div className="w-[220px]">
                  <Label>Editor</Label>
                  <Select
                    value={editorPreference}
                    onValueChange={(value) =>
                      setComposeEditor(value as "codemirror-vim" | "tiptap")
                    }
                  >
                    <SelectTrigger className="mt-1 h-9 bg-background" aria-label="Compose editor">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="tiptap">Rich text</SelectItem>
                      <SelectItem value="codemirror-vim">Markdown + vim</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <div className="flex min-w-[260px] flex-1 items-center justify-end gap-3">
                <AttachmentList
                  attachments={draft.frontmatter.attach}
                  onRemove={removeAttachment}
                />
                <ComposeActionButton
                  variant="outline"
                  size="sm"
                  onClick={handleAttachShortcut}
                  disabled={uploading > 0}
                  shortcut="⇧⌘A"
                >
                  {uploading > 0 ? (
                    <Loader2 className="size-3 animate-spin" />
                  ) : (
                    <Paperclip className="size-3" />
                  )}
                  Attach
                </ComposeActionButton>
              </div>
            </div>
          </div>

          <div className="px-3 py-3">
            <Suspense
              fallback={
                <div className="h-[520px] rounded-lg border border-border bg-surface p-4 text-xs text-muted-foreground">
                  Loading editor...
                </div>
              }
            >
              {editorPreference === "tiptap" ? (
                <TiptapComposeEditor
                  value={draft.bodyMarkdown}
                  onChange={updateBody}
                  onSave={handleSaveClick}
                  onSend={requestSend}
                  onDiscard={requestDiscard}
                />
              ) : (
                <CodeMirrorComposeEditor
                  value={draft.bodyMarkdown}
                  onChange={updateBody}
                  onSave={handleSaveClick}
                  onSend={requestSend}
                  onDiscard={requestDiscard}
                />
              )}
            </Suspense>
          </div>

          <footer className="flex flex-wrap items-center gap-3 border-t border-border px-3 py-2 text-xs text-muted-foreground">
            <span className={dirty ? "text-warning" : "text-success"}>{saveStatus}</span>
            <DraftQualityBadges suggestion={draftSuggestion} compact />
            {saveError ? <span className="text-destructive">{saveError}</span> : null}
            <span className="ml-auto font-mono text-2xs">
              Cmd-S save · Cmd-Enter send · Cmd-Backspace discard
            </span>
          </footer>
        </div>

        <IssueList issues={visibleIssues} />
      </div>

      <SendConfirmDialog
        open={sendConfirmOpen}
        onOpenChange={setSendConfirmOpen}
        recipientCount={recipientCount}
        account={selectedAccount}
        subject={draft.frontmatter.subject}
        suggestion={draftSuggestion}
        sending={sendSession.isPending || updateSession.isPending}
        onConfirm={confirmSend}
      />
      <DiscardConfirmDialog
        open={discardConfirmOpen}
        onOpenChange={setDiscardConfirmOpen}
        discarding={discardSession.isPending}
        onConfirm={discardDraft}
      />
    </div>
  );
}

function ComposeLoading({ title }: { title: string }) {
  return (
    <div className="flex flex-1 items-start justify-center bg-background px-4 py-5">
      <div className="w-full max-w-[980px]">
        <div className="mb-4 h-12 animate-pulse rounded-xl bg-muted" />
        <div className="h-[640px] animate-pulse rounded-2xl border border-border bg-muted/70" />
        <div className="mt-3 font-mono text-2xs text-muted-foreground">
          Opening {title.toLowerCase()}...
        </div>
      </div>
    </div>
  );
}

function ComposeActionButton({
  className,
  children,
  shortcut,
  ...props
}: ComponentProps<typeof Button> & { shortcut: string }) {
  return (
    <Button className={cn("gap-1.5", className)} title={shortcut} {...props}>
      {children}
      <ShortcutKbd>{shortcut}</ShortcutKbd>
    </Button>
  );
}

function ShortcutKbd({ children }: { children: string }) {
  return (
    <kbd className="ml-1 rounded border border-border/80 bg-background/70 px-1.5 py-0.5 font-mono text-[10px] font-medium text-muted-foreground">
      {children}
    </kbd>
  );
}

function AccountSelect({
  accounts,
  value,
  onChange,
}: {
  accounts: RuntimeAccount[];
  value: string;
  onChange: (value: string) => void;
}) {
  if (accounts.length === 0) {
    return (
      <div className="mt-1 rounded-md border border-border bg-background px-2 py-2 text-xs text-muted-foreground">
        {value || "Default account"}
      </div>
    );
  }
  return (
    <Select value={value || accounts[0]?.account_id} onValueChange={onChange}>
      <SelectTrigger className="mt-1 h-9 bg-background" aria-label="Compose account">
        <SelectValue placeholder="Select account" />
      </SelectTrigger>
      <SelectContent>
        {accounts.map((account) => (
          <SelectItem key={account.account_id} value={account.account_id}>
            {account.name || account.email} · {account.email}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function AddressField({
  label,
  value,
  inputRef,
  onChange,
}: {
  label: string;
  value: string;
  inputRef?: Ref<HTMLInputElement>;
  onChange: (value: string) => void;
}) {
  const chips = splitAddresses(value);
  const id = `compose-${label.toLowerCase()}`;
  return (
    <div className="grid grid-cols-[42px_1fr] items-start gap-2">
      <Label htmlFor={id} className="pt-2 text-muted-foreground">
        {label}
      </Label>
      <div>
        <Input
          id={id}
          ref={inputRef}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder={`${label}: name@example.com, teammate@example.com`}
          className="h-9 bg-background text-sm"
        />
        {chips.length > 0 ? (
          <div className="mt-2 flex flex-wrap gap-1.5">
            {chips.map((chip) => (
              <Badge key={chip} variant="secondary" className="py-1 text-foreground">
                {chip}
                <button
                  type="button"
                  className="rounded-full text-muted-foreground hover:text-foreground"
                  onClick={() => onChange(chips.filter((item) => item !== chip).join(", "))}
                  onKeyDown={(event) => {
                    if (event.key !== "Backspace" && event.key !== "Delete") return;
                    event.preventDefault();
                    onChange(chips.filter((item) => item !== chip).join(", "));
                  }}
                  aria-label={`Remove ${chip}`}
                >
                  <X className="size-3" />
                </button>
              </Badge>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function AttachmentList({
  attachments,
  onRemove,
}: {
  attachments: string[];
  onRemove: (path: string) => void;
}) {
  if (attachments.length === 0) {
    return <div className="text-2xs text-muted-foreground">No attachments</div>;
  }
  return (
    <div className="flex flex-1 flex-wrap gap-1.5">
      {attachments.map((path) => (
        <Badge
          key={path}
          variant="outline"
          className="max-w-full bg-background py-1 text-foreground"
        >
          <Paperclip className="size-3 text-muted-foreground" />
          <span className="max-w-[220px] truncate" title={path}>
            {basename(path)}
          </span>
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground"
            onClick={() => onRemove(path)}
            aria-label={`Remove ${basename(path)}`}
          >
            <X className="size-3" />
          </button>
        </Badge>
      ))}
    </div>
  );
}

function DraftQualityBadges({
  suggestion,
  compact = false,
}: {
  suggestion: DraftSuggestionResponse | null;
  compact?: boolean;
}) {
  if (!suggestion) return null;
  return (
    <div className={cn("flex flex-wrap items-center gap-1.5", compact && "text-2xs")}>
      {suggestion.voice_match ? (
        <Badge variant="outline" className="bg-background">
          Voice {Math.round(suggestion.voice_match.score * 100)}% ·{" "}
          {suggestion.voice_match.confidence}
        </Badge>
      ) : null}
      {suggestion.humanizer ? (
        <Badge variant="outline" className="bg-background">
          Humanizer {suggestion.humanizer.score}/100
        </Badge>
      ) : null}
      {suggestion.rewrite_iterations > 0 ? (
        <Badge variant="secondary">Rewritten {suggestion.rewrite_iterations}x</Badge>
      ) : null}
    </div>
  );
}

function IssueList({ issues }: { issues: ComposeIssue[] }) {
  if (issues.length === 0) return null;
  return (
    <div className="mt-3 space-y-2">
      {issues.map((issue) => (
        <Alert
          key={`${issue.severity}-${issue.message}`}
          variant={issue.severity === "error" ? "destructive" : "warning"}
          className="flex items-center gap-2 px-3 py-2"
        >
          <AlertTriangle
            className={
              issue.severity === "error"
                ? "size-3 shrink-0 text-destructive"
                : "size-3 shrink-0 text-warning"
            }
          />
          <AlertDescription>{issue.message}</AlertDescription>
        </Alert>
      ))}
    </div>
  );
}

function SendConfirmDialog({
  open,
  onOpenChange,
  recipientCount,
  account,
  subject,
  suggestion,
  sending,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  recipientCount: number;
  account?: RuntimeAccount;
  subject: string;
  suggestion: DraftSuggestionResponse | null;
  sending: boolean;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent
        onKeyDown={(event) => {
          if (event.key === "Enter" && !sending) {
            event.preventDefault();
            onConfirm();
          }
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Send message?</AlertDialogTitle>
          <AlertDialogDescription>
            Send to {recipientCount} {recipientCount === 1 ? "recipient" : "recipients"} via{" "}
            {account?.email ?? "the selected account"}.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="rounded-lg border border-border bg-muted px-3 py-2 text-sm">
          {subject.trim() || "(no subject)"}
        </div>
        <DraftQualityBadges suggestion={suggestion} />
        <AlertDialogFooter>
          <AlertDialogCancel variant="outline" disabled={sending}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={sending}
            onClick={(event) => {
              event.preventDefault();
              onConfirm();
            }}
          >
            {sending ? <Loader2 className="size-3 animate-spin" /> : <Send className="size-3" />}
            Send
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function DiscardConfirmDialog({
  open,
  onOpenChange,
  discarding,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  discarding: boolean;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent
        onKeyDown={(event) => {
          if (event.key === "Enter" && !discarding) {
            event.preventDefault();
            onConfirm();
          }
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Discard draft?</AlertDialogTitle>
          <AlertDialogDescription>
            This deletes the local compose file and attachments for this draft.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel variant="outline" disabled={discarding}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={discarding}
            onClick={(event) => {
              event.preventDefault();
              onConfirm();
            }}
          >
            {discarding ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Trash2 className="size-3" />
            )}
            Discard
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
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

function composeIntent(pathname: string, search: ComposeSearch): ComposeIntent {
  const draftMatch = pathname.match(/^\/compose\/([^/]+)$/);
  const draftId = draftMatch?.[1] ? decodeURIComponent(draftMatch[1]) : undefined;
  if (draftId && draftId !== "new") {
    return { key: `draft:${draftId}`, title: "Saved draft", kind: "new", draftId };
  }
  const reply = typeof search.reply === "string" ? search.reply : undefined;
  const prefillTo = typeof search.to === "string" ? search.to : undefined;
  const prefillSubject = typeof search.subject === "string" ? search.subject : undefined;
  const mode =
    search.mode === "forward" || search.mode === "all" || search.mode === "single"
      ? search.mode
      : undefined;
  const kind: ComposeKind = reply
    ? mode === "forward"
      ? "forward"
      : mode === "all"
        ? "reply_all"
        : "reply"
    : "new";
  const title =
    kind === "forward"
      ? "Forward message"
      : kind === "reply_all"
        ? "Reply all"
        : kind === "reply"
          ? "Reply"
          : "New message";
  const prefillKey = [prefillTo?.trim() ?? "", prefillSubject?.trim() ?? ""].join("|");
  const composeKey = reply ?? (prefillKey || "new");
  return {
    key: `compose:${kind}:${composeKey}`,
    title,
    kind,
    messageId: reply,
    prefillTo,
    prefillSubject,
  };
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

function hasFiles(dataTransfer: DataTransfer): boolean {
  return Array.from(dataTransfer.types).includes("Files");
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

function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  for (let index = parts.length - 1; index >= 0; index -= 1) {
    const part = parts[index];
    if (part) return part;
  }
  return path;
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
