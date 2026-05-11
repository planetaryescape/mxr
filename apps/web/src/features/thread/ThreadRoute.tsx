import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Archive,
  Ban,
  Clock,
  FileText,
  Forward,
  Mail,
  MailOpen,
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
  modifyLabels,
  shellKey,
  summarizeThread,
} from "@/features/mailbox/api";
import { SnoozeDialog } from "@/features/mailbox/SnoozeDialog";
import { AttachmentActions } from "@/features/thread/AttachmentActions";
import { MailboxRoute } from "@/features/mailbox/MailboxRoute";
import { MessageBody } from "@/features/thread/MessageBody";
import type {
  MessageBodyView,
  MessageLabelView,
  MessageRowView,
  ShellResponse,
  ThreadResponse,
} from "@/features/mailbox/types";
import { useOptimisticMailMutation } from "@/features/mailbox/useOptimisticMailMutation";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
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

export function ThreadRoute() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const parts = pathname.split("/").filter(Boolean);
  const threadId = parts[parts.length - 1] ?? "";
  const mailboxPath = `/${parts.slice(0, -1).join("/")}`;
  return (
    <div className="flex min-h-0 min-w-0 flex-1 bg-background">
      <div className="hidden w-[520px] shrink-0 xl:w-[560px] 2xl:w-[600px] lg:flex">
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
  const openRail = useModals((state) => state.openRightRail);

  useEffect(() => {
    const paneState = useMailboxPane.getState();
    if (paneState.suppressNextReaderFocus) {
      paneState.setSuppressNextReaderFocus(false);
      return;
    }
    setActivePane("reader");
  }, [data.thread.id, setActivePane]);

  const summary = useMutation({
    mutationFn: () => summarizeThread(data.thread.id),
    onSuccess: (result) => openRail("thread-summary", result),
    onError: (error) => toast.error("Summary failed", { description: error.message }),
  });
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
      void navigate({
        to: "/compose/new",
        search: { reply: primaryMessage.id, mode: composeMode },
      });
    },
    [navigate, primaryMessage],
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
        summary.mutate();
      } else if (event.key === "p") {
        event.preventDefault();
        if (primarySenderEmail) senderProfile.mutate(primarySenderEmail);
      } else if (event.key === "l") {
        event.preventDefault();
        setLabelDialogOpen(true);
      } else if (event.key.toLowerCase() === "z") {
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
    compose,
  ]);

  useEffect(() => {
    const unreadIds = data.messages.flatMap((message) => (message.unread ? [message.id] : []));
    if (unreadIds.length === 0) return;
    const handle = window.setTimeout(() => markReadRef.current(unreadIds), 2000);
    return () => window.clearTimeout(handle);
  }, [data.messages]);

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
        shortcut: "z",
        icon: <Clock className="size-3" />,
        onSelect: () => setSnoozeOpen(true),
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
      senderProfile,
      spam,
      toggleRead,
      toggleStar,
      trash,
    ],
  );

  return (
    <article
      aria-label="Thread reader"
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background"
      data-active-pane={activePane === "reader" ? "true" : undefined}
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
              className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-card px-2.5 text-xs font-medium text-muted-foreground shadow-sm"
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
          className="mt-3 flex min-w-0 flex-nowrap items-center gap-2 overflow-hidden border-t border-border/70 pt-3"
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
            onClick={() => summary.mutate()}
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
          {data.messages.map((message) => (
            <ThreadMessage
              key={message.id}
              message={message}
              body={bodiesByMessage.get(message.id)}
              mode={mode}
              remoteImages={remoteImages}
              emailHtmlTheme={emailHtmlTheme}
            />
          ))}
        </div>
      </div>
    </article>
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
}: {
  message: MessageRowView;
  body?: MessageBodyView;
  mode: "reader" | "plain" | "html";
  remoteImages: boolean;
  emailHtmlTheme: "dark" | "original";
}) {
  const plain = body?.reader_text ?? body?.text_plain ?? message.snippet;
  const html = body?.text_html;
  const attachments = body?.attachments ?? [];
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
      <div className="pb-6 text-[15px] leading-7">
        {mode === "html" && html ? (
          <MessageBody html={html} allowRemoteImages={remoteImages} theme={emailHtmlTheme} />
        ) : (
          <pre className="w-full whitespace-pre-wrap break-words font-sans text-[15px] leading-7 text-foreground">
            {plain || "No readable body."}
          </pre>
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

function formatAddressList(addresses?: { name?: string | null; email: string }[]): string {
  if (!addresses || addresses.length === 0) return "undisclosed recipients";
  return addresses.map(formatAddress).join(", ");
}

function formatAddress(address: { name?: string | null; email: string }): string {
  const name = address.name?.trim();
  return name ? `${name} <${address.email}>` : address.email;
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
