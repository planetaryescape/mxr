import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { find as findLinks } from "linkifyjs";
import {
  Archive,
  Ban,
  ChevronDown,
  Clock,
  FileText,
  Forward,
  Mail,
  MailOpen,
  Maximize2,
  Minimize2,
  MoreVertical,
  Paperclip,
  Reply,
  ReplyAll,
  RefreshCw,
  Star,
  Tag,
  Trash2,
  UserRound,
  Check,
} from "lucide-react";
import {
  type ComponentProps,
  type CSSProperties,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { toast } from "sonner";

import {
  fetchSenderProfile,
  fetchThread,
  listCommitments,
  modifyLabels,
  shellKey,
  summarizeThread,
} from "@/features/mailbox/api";
import { SnoozeDialog } from "@/features/mailbox/SnoozeDialog";
import { AttachmentActions } from "@/features/thread/AttachmentActions";
import { InviteCard } from "@/features/thread/InviteCard";
import { MailboxRoute } from "@/features/mailbox/MailboxRoute";
import { MessageBody } from "@/features/thread/MessageBody";
import type {
  MailboxResponse,
  MessageBodyView,
  MessageLabelView,
  MessageRowView,
  ShellResponse,
  ThreadResponse,
} from "@/features/mailbox/types";
import { useOptimisticMailMutation } from "@/features/mailbox/useOptimisticMailMutation";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { EmptyState } from "@/components/EmptyState";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

interface LabelChange {
  add: string[];
  remove: string[];
}

interface ThreadSummaryView {
  model?: string;
  generatedAt?: string;
  text: string;
  bullets: string[];
}

interface ThreadCommitmentView {
  id: string;
  direction: string;
  whoOwes: string;
  what: string;
  byWhen?: string | null;
}

export function ThreadRoute() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const readerLayout = useUiPrefs((state) => state.readerLayout);
  const parts = pathname.split("/").filter(Boolean);
  const threadId = parts[parts.length - 1] ?? "";
  const mailboxPath = `/${parts.slice(0, -1).join("/")}`;
  const readerFull = readerLayout === "full";
  return (
    <div className="flex min-h-0 min-w-0 flex-1 bg-background">
      <div
        className={cn(
          "hidden w-[520px] shrink-0 xl:w-[560px] 2xl:w-[600px]",
          readerFull ? "lg:hidden" : "lg:flex",
        )}
      >
        <MailboxRoute />
      </div>
      <ThreadReader threadId={threadId} mailboxPath={mailboxPath} />
    </div>
  );
}

function ThreadReader({ threadId, mailboxPath }: { threadId: string; mailboxPath: string }) {
  const query = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    enabled: Boolean(threadId),
  });

  if (query.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-xs text-muted-foreground">
        Loading thread…
      </div>
    );
  }

  if (query.isError) {
    return (
      <EmptyState
        role="alert"
        icon={RefreshCw}
        title="Thread unavailable"
        description={query.error.message}
        action={<Button onClick={() => query.refetch()}>Retry</Button>}
      />
    );
  }

  if (!query.data) return null;
  return <ThreadContent data={query.data} mailboxPath={mailboxPath} />;
}

function ThreadContent({ data, mailboxPath }: { data: ThreadResponse; mailboxPath: string }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const scrollRef = useRef<HTMLDivElement>(null);
  const activePane = useMailboxPane((state) => state.activePane);
  const setActivePane = useMailboxPane((state) => state.setActivePane);
  const setSuppressNextReaderFocus = useMailboxPane((state) => state.setSuppressNextReaderFocus);
  const emailHtmlTheme = useUiPrefs((state) => state.emailHtmlTheme);
  const readerLayout = useUiPrefs((state) => state.readerLayout);
  const setReaderLayout = useUiPrefs((state) => state.setReaderLayout);
  const shell = useShellQuery();
  const archive = useOptimisticMailMutation("archive");
  const spam = useOptimisticMailMutation("spam");
  const trash = useOptimisticMailMutation("trash");
  const star = useOptimisticMailMutation("star");
  const unstar = useOptimisticMailMutation("unstar");
  const markUnread = useOptimisticMailMutation("unread");
  const markReadAction = useOptimisticMailMutation("read");
  const markRead = useOptimisticMailMutation("read", { silentSuccess: true });
  const markReadRef = useRef(markRead.mutate);
  markReadRef.current = markRead.mutate;
  const [mode, setMode] = useState<"reader" | "plain" | "html">("html");
  const [remoteImages, setRemoteImages] = useState(true);
  const [snoozeOpen, setSnoozeOpen] = useState(false);
  const [labelDialogOpen, setLabelDialogOpen] = useState(false);
  const [threadSummary, setThreadSummary] = useState<ThreadSummaryView | null>(null);
  const [summaryExpanded, setSummaryExpanded] = useState(true);
  const autoSummaryThreadRef = useRef<string | null>(null);
  const openRail = useModals((state) => state.openRightRail);
  const closeRail = useModals((state) => state.closeRightRail);

  useEffect(() => {
    const paneState = useMailboxPane.getState();
    if (paneState.suppressNextReaderFocus) {
      paneState.setSuppressNextReaderFocus(false);
      return;
    }
    setActivePane("reader");
  }, [data.thread.id, setActivePane]);

  const summary = useMutation({
    mutationFn: (_input?: { silent?: boolean }) => summarizeThread(data.thread.id),
    onSuccess: (result, input) => {
      const view = normalizeThreadSummary(result);
      if (!view) {
        if (!input?.silent) {
          toast.error("Summary failed", { description: "The summary response was empty." });
        }
        return;
      }
      setThreadSummary(view);
      setSummaryExpanded(true);
      closeRail();
    },
    onError: (error, input) => {
      if (!input?.silent) {
        toast.error("Summary failed", { description: error.message });
      }
    },
  });
  const summaryMutateRef = useRef(summary.mutate);
  summaryMutateRef.current = summary.mutate;
  const senderProfile = useMutation({
    mutationFn: (email: string) => fetchSenderProfile({ accountId: data.thread.account_id, email }),
    onSuccess: (result) => openRail("sender-profile", result),
    onError: (error) => toast.error("Sender profile failed", { description: error.message }),
  });
  const bodiesByMessage = useMemo(
    () => new Map(data.bodies.map((body) => [body.message_id, body])),
    [data.bodies],
  );
  const allMessageIds = useMemo(() => data.messages.map((message) => message.id), [data.messages]);
  const attachments = data.bodies.flatMap((body) => body.attachments ?? []);
  const primaryMessage = data.messages[0];
  const anyUnread = data.messages.some((message) => message.unread);
  const anyStarred = data.messages.some((message) => message.starred);
  const recipientCount =
    (primaryMessage?.to?.length ?? 0) +
    (primaryMessage?.cc?.length ?? 0) +
    (primaryMessage?.bcc?.length ?? 0);
  const canReplyAll = recipientCount > 1 || data.thread.participants.length > 2;
  const threadLabels = useMemo(() => uniqueLabels(data.messages), [data.messages]);
  const labelOptions = useMemo(
    () => labelOptionsFromShell(shell.data, threadLabels),
    [shell.data, threadLabels],
  );
  const primarySenderEmail = extractEmail(primaryMessage?.sender_detail ?? primaryMessage?.sender);
  const commitments = useQuery({
    queryKey: ["commitments", data.thread.account_id, primarySenderEmail],
    queryFn: () =>
      listCommitments({
        accountId: data.thread.account_id,
        email: primarySenderEmail ?? undefined,
        status: "open",
      }),
    enabled: Boolean(primarySenderEmail),
    staleTime: 30_000,
  });
  const openCommitments = useMemo(
    () => extractThreadCommitments(commitments.data),
    [commitments.data],
  );
  const readerFull = readerLayout === "full";
  const toggleReaderLayout = useCallback(() => {
    setReaderLayout(readerFull ? "split" : "full");
  }, [readerFull, setReaderLayout]);
  useShortcutScope("thread", activePane === "reader");
  // Sibling threads come from the already-cached mailbox list (the split
  // pane keeps that query warm), so [ / ] can archive-and-advance. Read the
  // cache lazily — the list containing this thread is the active lens.
  const siblingThreadIds = useCallback((): string[] => {
    const cached = queryClient.getQueriesData<{ pages?: MailboxResponse[] }>({
      queryKey: ["mailbox"],
    });
    for (const [, value] of cached) {
      const pages = value?.pages ?? [];
      const ids = [
        ...new Set(
          pages.flatMap((page) =>
            page.mailbox.groups.flatMap((group) => group.rows.map((row) => row.thread_id)),
          ),
        ),
      ];
      if (ids.includes(data.thread.id)) return ids;
    }
    return [];
  }, [data.thread.id, queryClient]);
  const archiveAndStep = useCallback(
    (delta: 1 | -1) => {
      if (allMessageIds.length === 0) return;
      const siblings = siblingThreadIds();
      const index = siblings.indexOf(data.thread.id);
      const nextId = index >= 0 ? siblings[index + delta] : undefined;
      archive.mutate(allMessageIds);
      if (nextId) {
        void navigate({ to: `${mailboxPath}/${nextId}` });
      } else {
        void navigate({ to: mailboxPath });
      }
    },
    [allMessageIds, archive, data.thread.id, mailboxPath, navigate, siblingThreadIds],
  );
  const labelMutation = useMutation({
    mutationFn: ({ add, remove }: LabelChange) => modifyLabels(allMessageIds, add, remove),
    onSuccess: (response) => {
      const count = response.result?.succeeded ?? allMessageIds.length;
      toast.success(`Updated labels for ${count} ${count === 1 ? "message" : "messages"}`);
      setLabelDialogOpen(false);
    },
    onError: (error) => toast.error("Label update failed", { description: error.message }),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ["mailbox"] });
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      void queryClient.invalidateQueries({ queryKey: shellKey });
    },
  });

  const compose = useCallback(
    (composeMode: "single" | "all" | "forward") => {
      if (!primaryMessage) return;
      // Reply opens inline at the bottom of the thread; the host keeps the
      // session alive if the user pops it out or goes fullscreen.
      useComposeUi.getState().openCompose(replyIntent(primaryMessage.id, composeMode), "inline");
    },
    [primaryMessage],
  );

  const toggleStar = useCallback(() => {
    if (!primaryMessage) return;
    (anyStarred ? unstar : star).mutate([primaryMessage.id]);
  }, [anyStarred, primaryMessage, star, unstar]);

  const toggleRead = useCallback(() => {
    if (allMessageIds.length === 0) return;
    (anyUnread ? markReadAction : markUnread).mutate(allMessageIds);
  }, [allMessageIds, anyUnread, markReadAction, markUnread]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (activePane !== "reader") return;
      const target = event.target;
      if (target instanceof HTMLElement) {
        if (target.closest("input, textarea, select, [contenteditable=true]")) return;
      }
      if (labelDialogOpen || snoozeOpen) return;
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        scrollRef.current?.scrollBy({ top: 72, behavior: "smooth" });
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        scrollRef.current?.scrollBy({ top: -72, behavior: "smooth" });
      } else if (event.key === "h" || event.key === "ArrowLeft") {
        event.preventDefault();
        setSuppressNextReaderFocus(true);
        setActivePane("mailbox");
      } else if (event.key === "u" || event.key === "Escape") {
        event.preventDefault();
        void navigate({ to: mailboxPath });
      } else if (event.key === "e") {
        event.preventDefault();
        archive.mutate(allMessageIds);
        void navigate({ to: mailboxPath });
      } else if (event.key === "]") {
        event.preventDefault();
        archiveAndStep(1);
      } else if (event.key === "[") {
        event.preventDefault();
        archiveAndStep(-1);
      } else if (event.key === "s") {
        event.preventDefault();
        toggleStar();
      } else if (event.key === "m") {
        event.preventDefault();
        toggleRead();
      } else if (event.key === "L") {
        event.preventDefault();
        openRail("thread-context", data.right_rail);
      } else if (event.key === "A") {
        event.preventDefault();
        if (attachments.length > 0) openRail("attachments", attachments);
      } else if (event.key === "y") {
        event.preventDefault();
        summary.mutate(undefined);
      } else if (event.key === "p") {
        event.preventDefault();
        if (primarySenderEmail) senderProfile.mutate(primarySenderEmail);
      } else if (event.key === "F") {
        event.preventDefault();
        toggleReaderLayout();
      } else if (event.key === "l") {
        event.preventDefault();
        setLabelDialogOpen(true);
      } else if (event.key === "Z") {
        // Shift+Z snoozes (matches the TUI); lowercase z stays free for the
        // global undo binding.
        event.preventDefault();
        setSnoozeOpen(true);
      } else if (event.key === "r") {
        event.preventDefault();
        compose("single");
      } else if (event.key === "a") {
        event.preventDefault();
        if (canReplyAll) compose("all");
      } else if (event.key === "f") {
        event.preventDefault();
        compose("forward");
      } else if (event.key === "!") {
        event.preventDefault();
        spam.mutate(allMessageIds);
        void navigate({ to: mailboxPath });
      } else if (event.key === "Delete" || event.key === "Backspace") {
        event.preventDefault();
        trash.mutate(allMessageIds);
        void navigate({ to: mailboxPath });
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    activePane,
    allMessageIds,
    archive,
    attachments,
    canReplyAll,
    data.right_rail,
    mailboxPath,
    navigate,
    labelDialogOpen,
    openRail,
    primarySenderEmail,
    setActivePane,
    setSuppressNextReaderFocus,
    senderProfile,
    snoozeOpen,
    spam,
    summary,
    trash,
    toggleRead,
    toggleStar,
    toggleReaderLayout,
    compose,
    archiveAndStep,
  ]);

  useEffect(() => {
    const unreadIds = data.messages.flatMap((message) => (message.unread ? [message.id] : []));
    if (unreadIds.length === 0) return;
    const handle = window.setTimeout(() => markReadRef.current(unreadIds), 2000);
    return () => window.clearTimeout(handle);
  }, [data.messages]);

  useEffect(() => {
    const cachedSummary = normalizeThreadSummary(data.summary ? { summary: data.summary } : null);
    setThreadSummary(cachedSummary);
    setSummaryExpanded(true);
    if (cachedSummary) {
      autoSummaryThreadRef.current = data.thread.id;
      return;
    }
    if (autoSummaryThreadRef.current === data.thread.id) return;
    // Trailing-edge debounce: scrolling through a list and briefly
    // landing on threads shouldn't fire an LLM call for each one.
    // Only the thread the user actually stays on for ~250ms triggers
    // the request. Switching threads before the timer elapses cancels
    // this pending fire; in-flight requests already sent are left to
    // complete so the daemon can cache their result.
    const threadId = data.thread.id;
    const handle = window.setTimeout(() => {
      autoSummaryThreadRef.current = threadId;
      summaryMutateRef.current({ silent: true });
    }, 250);
    return () => window.clearTimeout(handle);
  }, [data.thread.id, data.summary]);

  const overflowActions = useMemo<ReaderOverflowAction[]>(
    () => [
      {
        label: anyStarred ? "Unstar" : "Star",
        shortcut: "s",
        icon: <Star className={cn("size-3", anyStarred && "fill-current text-star")} />,
        onSelect: toggleStar,
      },
      {
        label: "Archive",
        shortcut: "e",
        icon: <Archive className="size-3" />,
        onSelect: () => archive.mutate(allMessageIds),
      },
      {
        label: "Spam",
        shortcut: "!",
        icon: <Ban className="size-3" />,
        onSelect: () => spam.mutate(allMessageIds),
      },
      {
        label: "Trash",
        shortcut: "Del",
        icon: <Trash2 className="size-3" />,
        destructive: true,
        onSelect: () => trash.mutate(allMessageIds),
      },
      {
        label: anyUnread ? "Mark read" : "Mark unread",
        shortcut: "m",
        icon: anyUnread ? <MailOpen className="size-3" /> : <Mail className="size-3" />,
        onSelect: toggleRead,
      },
      {
        label: "Labels",
        shortcut: "l",
        icon: <Tag className="size-3" />,
        onSelect: () => setLabelDialogOpen(true),
      },
      {
        label: "Snooze",
        shortcut: "Z",
        icon: <Clock className="size-3" />,
        onSelect: () => setSnoozeOpen(true),
      },
      {
        label: readerFull ? "Split reader" : "Full reader",
        shortcut: "F",
        icon: readerFull ? <Minimize2 className="size-3" /> : <Maximize2 className="size-3" />,
        onSelect: toggleReaderLayout,
      },
      {
        label: "Context",
        shortcut: "L",
        icon: <UserRound className="size-3" />,
        onSelect: () => openRail("thread-context", data.right_rail),
      },
      {
        label: "Sender",
        shortcut: "p",
        icon: <UserRound className="size-3" />,
        disabled: !primarySenderEmail || senderProfile.isPending,
        onSelect: () => primarySenderEmail && senderProfile.mutate(primarySenderEmail),
      },
      ...(attachments.length > 0
        ? [
            {
              label: `Attachments (${attachments.length})`,
              shortcut: "A",
              icon: <Paperclip className="size-3" />,
              onSelect: () => openRail("attachments", attachments),
            },
          ]
        : []),
    ],
    [
      allMessageIds,
      anyStarred,
      anyUnread,
      archive,
      attachments,
      data.right_rail,
      openRail,
      primarySenderEmail,
      readerFull,
      senderProfile,
      spam,
      toggleRead,
      toggleReaderLayout,
      toggleStar,
      trash,
    ],
  );

  return (
    <article
      aria-label="Thread reader"
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background"
      data-active-pane={activePane === "reader" ? "true" : undefined}
      data-reader-layout={readerLayout}
      onMouseDown={() => {
        setSuppressNextReaderFocus(false);
        setActivePane("reader");
      }}
    >
      <header className="border-b border-border px-5 py-3 lg:px-6">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="text-pretty text-lg font-semibold tracking-tight">
                {data.thread.subject || "(no subject)"}
              </h1>
              {threadLabels.map((label) => (
                <LabelBadge key={label.id} label={label} />
              ))}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <span>{data.thread.message_count} messages</span>
              <span>·</span>
              <span>{data.thread.unread_count} unread</span>
              <span>·</span>
              <span>
                {data.thread.participants
                  .map((p) => p.name || p.email)
                  .slice(0, 4)
                  .join(", ")}
              </span>
            </div>
          </div>
          <div className="flex shrink-0 flex-wrap items-center justify-end gap-2.5">
            <div className="flex items-center gap-1.5">
              <span className="text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
                View
              </span>
              <ToggleGroup
                type="single"
                value={mode}
                onValueChange={(value) => {
                  if (value) setMode(value as typeof mode);
                }}
                aria-label="Message body view"
              >
                {(["html", "reader", "plain"] as const).map((viewMode) => (
                  <ToggleGroupItem key={viewMode} value={viewMode} size="sm">
                    {mode === viewMode ? <Check className="size-3" /> : null}
                    {viewMode === "html" ? "HTML" : viewMode === "reader" ? "Reader" : "Plain"}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </div>
            <label
              className="inline-flex h-8 items-center gap-2 rounded-md border border-border/90 bg-muted/70 px-2.5 text-xs font-medium text-foreground shadow-sm"
              htmlFor="thread-remote-images"
            >
              <span>Remote images</span>
              <Switch
                id="thread-remote-images"
                checked={remoteImages}
                onCheckedChange={setRemoteImages}
                aria-label="Remote images"
              />
            </label>
          </div>
        </div>
        <div
          className="mt-3 flex min-w-0 flex-nowrap items-center justify-end gap-2 overflow-hidden border-t border-border/70 pt-3"
          role="toolbar"
          aria-label="Message actions"
        >
          <ReaderActionButton onClick={() => compose("single")} shortcut="r">
            <Reply className="size-3" />
            Reply
          </ReaderActionButton>
          {canReplyAll ? (
            <ReaderActionButton onClick={() => compose("all")} shortcut="a">
              <ReplyAll className="size-3" />
              Reply all
            </ReaderActionButton>
          ) : null}
          <ReaderActionButton onClick={() => compose("forward")} shortcut="f">
            <Forward className="size-3" />
            Forward
          </ReaderActionButton>
          <ReaderActionButton
            onClick={() => summary.mutate(undefined)}
            disabled={summary.isPending}
            shortcut="y"
          >
            <FileText className="size-3" />
            {summary.isPending ? "Summarizing..." : "Summary"}
          </ReaderActionButton>
          <ReaderActionMenu actions={overflowActions} />
        </div>
      </header>

      <SnoozeDialog
        open={snoozeOpen}
        messageIds={allMessageIds}
        onOpenChange={setSnoozeOpen}
        onSnoozed={() => void navigate({ to: mailboxPath })}
      />
      <ThreadLabelDialog
        open={labelDialogOpen}
        labels={labelOptions}
        currentLabels={threadLabels}
        pending={labelMutation.isPending}
        onOpenChange={setLabelDialogOpen}
        onSubmit={(change) => {
          if (allMessageIds.length === 0) return;
          labelMutation.mutate(change);
        }}
      />

      <div
        ref={scrollRef}
        data-testid="thread-scroll"
        className="flex min-h-0 flex-1 flex-col overflow-auto px-4 py-3 sm:px-6 lg:px-8"
      >
        <div className="flex w-full min-w-0 flex-col">
          {threadSummary ? (
            <ThreadSummaryAccordion
              summary={threadSummary}
              expanded={summaryExpanded}
              onExpandedChange={setSummaryExpanded}
            />
          ) : summary.isPending ? (
            <ThreadSummaryLoading />
          ) : null}
          {openCommitments.length > 0 ? (
            <ThreadCommitmentChips commitments={openCommitments} />
          ) : null}
          {data.messages.map((message) => (
            <ThreadMessage
              key={message.id}
              message={message}
              body={bodiesByMessage.get(message.id)}
              mode={mode}
              remoteImages={remoteImages}
              emailHtmlTheme={emailHtmlTheme}
              threadId={data.thread.id}
            />
          ))}
          {/* ComposeHost portals the inline reply composer here. */}
          <div id="inline-composer-slot" className="mt-3 empty:hidden" />
        </div>
      </div>
    </article>
  );
}

function ThreadSummaryAccordion({
  summary,
  expanded,
  onExpandedChange,
}: {
  summary: ThreadSummaryView;
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
}) {
  return (
    <section className="mb-4 rounded-lg border border-border/80 bg-muted/35 shadow-sm">
      <button
        type="button"
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
        aria-expanded={expanded}
        onClick={() => onExpandedChange(!expanded)}
      >
        <span className="flex min-w-0 items-center gap-2">
          <FileText className="size-4 shrink-0 text-primary" />
          <span className="font-medium">AI overview</span>
          {summary.model ? (
            <span className="truncate font-mono text-2xs text-muted-foreground">
              {summary.model}
            </span>
          ) : null}
        </span>
        <ChevronDown
          className={cn(
            "size-4 shrink-0 text-muted-foreground transition-transform",
            expanded && "rotate-180",
          )}
        />
      </button>
      {expanded ? (
        <div className="break-words border-t border-border/70 px-4 py-3 text-sm leading-6 text-foreground">
          {summary.bullets.length > 0 ? (
            <ul className="space-y-1.5">
              {summary.bullets.map((item) => (
                <li key={item} className="flex gap-2">
                  <span className="mt-2 size-1.5 shrink-0 rounded-full bg-primary" />
                  <span className="min-w-0 break-words">{item}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="whitespace-pre-wrap break-words">{summary.text}</p>
          )}
        </div>
      ) : null}
    </section>
  );
}

function ThreadSummaryLoading() {
  return (
    <section className="mb-4 rounded-lg border border-border/80 bg-muted/25 px-4 py-3 text-sm text-muted-foreground">
      <span className="flex items-center gap-2">
        <RefreshCw className="size-3.5 animate-spin" />
        Summarizing thread…
      </span>
    </section>
  );
}

function ThreadCommitmentChips({ commitments }: { commitments: ThreadCommitmentView[] }) {
  return (
    <section
      aria-label="Open commitments"
      className="mb-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3"
    >
      <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
        <FileText className="size-3.5 text-amber-500" />
        Open commitments
      </div>
      <div className="flex flex-wrap gap-2">
        {commitments.slice(0, 4).map((commitment) => (
          <Badge
            key={commitment.id}
            variant="outline"
            className="max-w-full gap-1.5 border-amber-500/40 bg-background/70 py-1 text-2xs"
            title={commitment.what}
          >
            <span className="font-medium">{commitment.whoOwes}</span>
            <span className="text-muted-foreground">{commitment.direction}</span>
            <span className="max-w-[320px] truncate">{commitment.what}</span>
            {commitment.byWhen ? (
              <span className="text-muted-foreground">due {shortDate(commitment.byWhen)}</span>
            ) : null}
          </Badge>
        ))}
      </div>
    </section>
  );
}

function ThreadLabelDialog({
  open,
  labels,
  currentLabels,
  pending,
  onOpenChange,
  onSubmit,
}: {
  open: boolean;
  labels: MessageLabelView[];
  currentLabels: MessageLabelView[];
  pending: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (change: LabelChange) => void;
}) {
  const currentLabelIds = useMemo(
    () => currentLabels.filter(isAssignableLabel).map((label) => label.id),
    [currentLabels],
  );
  const currentLabelIdSet = useMemo(() => new Set(currentLabelIds), [currentLabelIds]);
  const [selectedLabelIds, setSelectedLabelIds] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    if (open) setSelectedLabelIds(new Set(currentLabelIds));
  }, [open, currentLabelIds]);

  const hasChanges = labels.some(
    (label) => selectedLabelIds.has(label.id) !== currentLabelIdSet.has(label.id),
  );

  function toggleLabel(labelId: string, checked: boolean) {
    setSelectedLabelIds((previous) => {
      const next = new Set(previous);
      if (checked) {
        next.add(labelId);
      } else {
        next.delete(labelId);
      }
      return next;
    });
  }

  function submit() {
    const add = labels
      .filter((label) => selectedLabelIds.has(label.id) && !currentLabelIdSet.has(label.id))
      .map((label) => label.name);
    const remove = labels
      .filter((label) => !selectedLabelIds.has(label.id) && currentLabelIdSet.has(label.id))
      .map((label) => label.name);
    if (add.length === 0 && remove.length === 0) return;
    onSubmit({ add, remove });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Edit labels</DialogTitle>
          <DialogDescription>
            Check labels to add them to this thread. Uncheck labels to remove them.
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-80 overflow-auto rounded-md border border-border bg-card p-1">
          {labels.length === 0 ? (
            <div className="px-3 py-6 text-sm text-muted-foreground">No labels available.</div>
          ) : (
            labels.map((label) => (
              <label
                key={label.id}
                className="flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm hover:bg-muted"
              >
                <Checkbox
                  checked={selectedLabelIds.has(label.id)}
                  onCheckedChange={(checked) => toggleLabel(label.id, checked === true)}
                />
                <span className="min-w-0 flex-1 truncate">{label.name}</span>
              </label>
            ))
          )}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            onClick={submit}
            disabled={pending || !hasChanges || labels.length === 0}
          >
            {pending ? "Applying..." : "Apply label changes"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface ReaderOverflowAction {
  label: string;
  shortcut?: string;
  icon: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
}

function ReaderActionMenu({ actions }: { actions: ReaderOverflowAction[] }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          className="h-9 w-9 shrink-0 rounded-md border-border/90 bg-muted/60 shadow-sm hover:border-primary/60 hover:bg-primary/15"
          aria-label="More message actions"
        >
          <MoreVertical className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        {actions.map((action) => (
          <DropdownMenuItem
            key={action.label}
            disabled={action.disabled}
            className={cn(action.destructive && "text-destructive focus:text-destructive")}
            onSelect={action.onSelect}
          >
            {action.icon}
            <span>{action.label}</span>
            {action.shortcut ? (
              <DropdownMenuShortcut>{action.shortcut}</DropdownMenuShortcut>
            ) : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function ReaderActionButton({
  className,
  children,
  shortcut,
  title,
  ...props
}: ComponentProps<typeof Button> & { shortcut?: string }) {
  const visibleShortcut = shortcut ?? (typeof title === "string" ? title : undefined);
  return (
    <Button
      variant="outline"
      size="sm"
      className={cn(
        "h-9 rounded-md border-border/90 bg-muted/60 px-3 text-xs shadow-sm hover:border-primary/60 hover:bg-primary/15",
        "shrink-0",
        className,
      )}
      title={title}
      {...props}
    >
      {children}
      {visibleShortcut ? (
        <>
          {" "}
          <kbd className="ml-1.5 rounded border border-border/80 bg-background/70 px-1.5 py-0.5 font-mono text-[10px] font-medium text-muted-foreground">
            {visibleShortcut}
          </kbd>
        </>
      ) : null}
    </Button>
  );
}

function ThreadMessage({
  message,
  body,
  mode,
  remoteImages,
  emailHtmlTheme,
  threadId,
}: {
  message: MessageRowView;
  body?: MessageBodyView;
  mode: "reader" | "plain" | "html";
  remoteImages: boolean;
  emailHtmlTheme: "dark" | "original";
  threadId: string;
}) {
  const plain = body?.reader_text ?? body?.text_plain ?? message.snippet;
  const html = body?.text_html;
  const attachments = body?.attachments ?? [];
  const calendar = body?.metadata?.calendar;
  return (
    <section
      className={cn(
        "min-w-0 border-b border-border bg-background",
        message.unread && "border-l-2 border-l-primary pl-4",
      )}
      data-testid="thread-message"
    >
      <div className="flex items-start justify-between gap-4 py-2.5">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <div className="break-words text-base font-medium">{message.sender}</div>
            {message.labels?.map((label) => (
              <LabelBadge key={label.id} label={label} />
            ))}
          </div>
          <div className="mt-1 break-all font-mono text-xs text-muted-foreground">
            {message.sender_detail ?? message.provider_id}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            to {formatAddressList(message.to)}
            {message.cc && message.cc.length > 0 ? ` · cc ${formatAddressList(message.cc)}` : null}
          </div>
        </div>
        <time
          className="shrink-0 text-right font-mono text-xs text-muted-foreground"
          dateTime={message.date}
          title={message.date_full}
        >
          {message.date_label}
          {message.date_relative ? (
            <span className="ml-1 text-muted-foreground/80">({message.date_relative})</span>
          ) : null}
        </time>
      </div>
      {calendar && (
        <InviteCard
          messageId={message.id}
          threadId={threadId}
          metadata={calendar}
        />
      )}
      <div className="pb-6 text-[15px] leading-7">
        {mode === "html" && html ? (
          <MessageBody html={html} allowRemoteImages={remoteImages} theme={emailHtmlTheme} />
        ) : (
          <LinkifiedPre text={plain || "No readable body."} />
        )}
      </div>
      {attachments.length > 0 ? (
        <div className="grid gap-2 border-t border-border py-4 sm:grid-cols-2">
          {attachments.map((attachment) => (
            <AttachmentActions
              key={attachment.id ?? attachment.filename}
              attachment={attachment}
              messageId={body?.message_id}
            />
          ))}
        </div>
      ) : null}
    </section>
  );
}

function LinkifiedPre({ text }: { text: string }) {
  const links = useMemo(() => findLinks(text, { defaultProtocol: "https" }), [text]);
  if (links.length === 0) {
    return (
      <pre className="w-full whitespace-pre-wrap break-words font-sans text-[15px] leading-7 text-foreground">
        {text}
      </pre>
    );
  }

  const nodes: ReactNode[] = [];
  let cursor = 0;
  links.forEach((link) => {
    if (link.start > cursor) nodes.push(text.slice(cursor, link.start));
    nodes.push(
      <a
        key={`${link.href}-${link.start}-${link.end}`}
        href={link.href}
        target="_blank"
        rel="noopener noreferrer"
        className="text-primary underline underline-offset-2 hover:text-primary/80"
      >
        {text.slice(link.start, link.end)}
      </a>,
    );
    cursor = link.end;
  });
  if (cursor < text.length) nodes.push(text.slice(cursor));

  return (
    <pre className="w-full whitespace-pre-wrap break-words font-sans text-[15px] leading-7 text-foreground">
      {nodes}
    </pre>
  );
}

function formatAddressList(addresses?: { name?: string | null; email: string }[]): string {
  if (!addresses || addresses.length === 0) return "undisclosed recipients";
  return addresses.map(formatAddress).join(", ");
}

function formatAddress(address: { name?: string | null; email: string }): string {
  const name = address.name?.trim();
  return name ? `${name} <${address.email}>` : address.email;
}

function normalizeThreadSummary(payload: unknown): ThreadSummaryView | null {
  const source =
    isRecord(payload) && isRecord(payload.summary)
      ? payload.summary
      : isRecord(payload)
        ? payload
        : null;
  const text = typeof source?.text === "string" ? source.text.trim() : "";
  if (!text) return null;
  const model = typeof source?.model === "string" ? source.model : undefined;
  const generatedAt = typeof source?.generated_at === "string" ? source.generated_at : undefined;
  return { generatedAt, model, text, bullets: summaryBullets(text) };
}

function extractThreadCommitments(payload: unknown): ThreadCommitmentView[] {
  const commitments =
    isRecord(payload) && Array.isArray(payload.commitments) ? payload.commitments : [];
  return commitments.flatMap((item) => {
    if (!isRecord(item)) return [];
    const id = typeof item.id === "string" ? item.id : "";
    const what = typeof item.what === "string" ? item.what.trim() : "";
    const whoOwes = typeof item.who_owes === "string" ? item.who_owes.trim() : "";
    const direction = typeof item.direction === "string" ? item.direction : "";
    if (!id || !what || !whoOwes || !direction) return [];
    return [
      {
        id,
        what,
        whoOwes,
        direction,
        byWhen: typeof item.by_when === "string" ? item.by_when : null,
      },
    ];
  });
}

function shortDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function summaryBullets(text: string): string[] {
  const lines = text
    .split(/\r?\n/)
    .map((line) =>
      line
        .trim()
        .replace(/^[-*•]\s+/, "")
        .replace(/^\d+[.)]\s+/, ""),
    )
    .filter((line) => !/^(summary|next steps):$/i.test(line))
    .filter(Boolean);
  if (lines.length > 1) return lines;
  return [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function uniqueLabels(messages: MessageRowView[]) {
  const labels = new Map<string, NonNullable<MessageRowView["labels"]>[number]>();
  for (const message of messages) {
    for (const label of message.labels ?? []) {
      labels.set(label.id, label);
    }
  }
  return [...labels.values()];
}

function labelOptionsFromShell(
  shell: ShellResponse | undefined,
  currentLabels: MessageLabelView[],
): MessageLabelView[] {
  const labels = new Map<string, MessageLabelView>();
  for (const section of shell?.sidebar?.sections ?? []) {
    if (section.id !== "labels") continue;
    for (const item of section.items) {
      const labelId = item.lens?.kind === "label" ? item.lens.labelId : undefined;
      if (!labelId) continue;
      labels.set(labelId, { id: labelId, name: item.label, kind: "user", color: null });
    }
  }
  for (const label of currentLabels) {
    if (isAssignableLabel(label)) labels.set(label.id, label);
  }
  return [...labels.values()];
}

function isAssignableLabel(label: MessageLabelView): boolean {
  return label.kind !== "system";
}

function LabelBadge({ label }: { label: MessageLabelView }) {
  const style = labelBadgeStyle(labelDisplayColor(label));
  return (
    <Badge variant={style ? "outline" : "secondary"} style={style} title={label.name}>
      {style ? (
        <span className="size-1.5 rounded-full" style={{ backgroundColor: style.color }} />
      ) : null}
      {label.name}
    </Badge>
  );
}

function labelDisplayColor(label: MessageLabelView): string | null {
  return normalizeHexColor(label.color) ?? fallbackLabelColor(label.name);
}

function labelBadgeStyle(color?: string | null): CSSProperties | undefined {
  const hex = normalizeHexColor(color);
  if (!hex) return undefined;
  return {
    backgroundColor: hexToRgba(hex, 0.16),
    borderColor: hexToRgba(hex, 0.65),
    color: hex,
  };
}

function normalizeHexColor(value?: string | null): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  const short = trimmed.match(/^#([0-9a-f]{3})$/i);
  if (short) {
    return `#${short[1]!
      .split("")
      .map((part) => part + part)
      .join("")}`;
  }
  const long = trimmed.match(/^#([0-9a-f]{6})$/i);
  return long ? `#${long[1]}` : null;
}

function fallbackLabelColor(name: string): string {
  switch (name.toUpperCase()) {
    case "INBOX":
      return "#60a5fa";
    case "STARRED":
    case "IMPORTANT":
      return "#facc15";
    case "SENT":
      return "#9ca3af";
    case "DRAFT":
      return "#d946ef";
    case "TRASH":
      return "#f87171";
    case "SPAM":
      return "#fb923c";
    case "ARCHIVE":
    case "ALL MAIL":
      return "#6b7280";
    default: {
      const colors = [
        "#60a5fa",
        "#34d399",
        "#fb923c",
        "#a78bfa",
        "#fb7185",
        "#38bdf8",
        "#fdba74",
        "#86efac",
      ];
      const hash = [...name].reduce((acc, char) => (acc + char.charCodeAt(0)) % 256, 0);
      return colors[hash % colors.length]!;
    }
  }
}

function hexToRgba(hex: string, alpha: number): string {
  const value = hex.slice(1);
  const red = Number.parseInt(value.slice(0, 2), 16);
  const green = Number.parseInt(value.slice(2, 4), 16);
  const blue = Number.parseInt(value.slice(4, 6), 16);
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

function extractEmail(value?: string | null): string | null {
  if (!value) return null;
  const angle = value.match(/<([^>]+@[^>]+)>/);
  if (angle?.[1]) return angle[1].trim();
  const bare = value.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i);
  return bare?.[0] ?? null;
}
