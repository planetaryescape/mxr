import { useMutation, useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { performMailAction } from "@/features/mail-actions/mailMutations";
import { createMailVerbs } from "@/features/mail-actions/mailVerbs";
import { useProjectedMessages } from "@/features/mail-actions/pendingMailOps";
import { targetFromThread } from "@/features/mail-actions/target";
import {
  fetchThread,
  listCommitments,
  resolveCommitment,
  summarizeThread,
} from "@/features/mailbox/api";
import { Centered } from "@/features/mailbox/MailViewParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import type { ThreadResponse } from "@/features/mailbox/types";
import { useLlmStatus } from "@/features/llm/useLlmStatus";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { parseAddress } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useOpenThread } from "@/state/openThreadStore";
import { useUiPrefs, type ReaderView } from "@/state/uiPrefsStore";

import { HeadersDialog } from "./HeadersDialog";
import { standaloneHtmlDocument } from "./MessageBody";
import { MessageCard } from "./MessageCard";
import { ThreadHeader } from "./ThreadHeader";
import {
  ThreadCommitmentChips,
  ThreadSummaryAccordion,
  ThreadSummaryLoading,
  extractThreadCommitments,
  normalizeThreadSummary,
  type ThreadSummaryView,
} from "./ThreadInsights";

/** The reader pane for one conversation, opened from a list (see MailView). */
export function ThreadPane({ threadId }: { threadId: string }) {
  const query = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    placeholderData: (previous) => previous,
  });
  const setOpenThread = useOpenThread((s) => s.setThreadId);
  useEffect(() => {
    setOpenThread(threadId);
    return () => setOpenThread(null);
  }, [setOpenThread, threadId]);

  if (query.isLoading) return <ReaderSkeleton />;
  if (query.isError) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't open this conversation"
          body={query.error.message}
          action={
            <Button size="sm" onClick={() => void query.refetch()}>
              Try again
            </Button>
          }
        />
      </div>
    );
  }
  if (!query.data) return null;
  // Key by thread so per-thread state (expanded, remote images) resets.
  return (
    <ThreadReader key={query.data.thread.id} data={query.data} stale={query.isPlaceholderData} />
  );
}

function ThreadReader({ data, stale }: { data: ThreadResponse; stale: boolean }) {
  const nav = useReaderNav();
  const scrollRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef(new Map<string, HTMLElement>());
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const readerLayout = useUiPrefs((s) => s.readerLayout);
  const setReaderLayout = useUiPrefs((s) => s.setReaderLayout);
  const defaultView = useUiPrefs((s) => s.readerView);
  const setDefaultView = useUiPrefs((s) => s.setReaderView);
  const messages = useProjectedMessages(data.messages);
  const bodies = useMemo(
    () => new Map(data.bodies.map((body) => [body.message_id, body])),
    [data.bodies],
  );

  const [view, setView] = useState<ReaderView>(defaultView);
  const [showQuotes, setShowQuotes] = useState(false);
  const [showSignature, setShowSignature] = useState(false);
  const [remoteAllowed, setRemoteAllowed] = useState(false);
  const [headersFor, setHeadersFor] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(() => initialExpanded(data));
  const [focusIndex, setFocusIndex] = useState(() =>
    Math.max(0, lastExpandedIndex(data, initialExpanded(data))),
  );
  const [summary, setSummary] = useState<ThreadSummaryView | null>(() =>
    normalizeThreadSummary(data.summary ? { summary: data.summary } : null),
  );

  const readerFocused = activePane === "reader";
  useShortcutScope("reader", readerFocused);

  // DOM focus follows the pane, so the page scrolls with the keyboard.
  useEffect(() => {
    if (!readerFocused) return;
    const active = document.activeElement;
    if (
      active &&
      active !== document.body &&
      active.closest("input, textarea, [contenteditable=true], [role=dialog]")
    )
      return;
    scrollRef.current?.focus({ preventScroll: true });
  }, [readerFocused]);

  // Land on the newest unread message, like a mail client, not the top.
  useEffect(() => {
    const target = messages[focusIndex];
    const node = target ? cardRefs.current.get(target.id) : undefined;
    if (node && focusIndex > 0) node.scrollIntoView({ block: "start" });
    // Only on first render of this thread.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Mark read after a short dwell: at once when the reader has focus, after
  // two seconds when the list is previewing threads under the cursor.
  useEffect(() => {
    const unread = messages.filter((message) => message.unread).map((message) => message.id);
    if (unread.length === 0) return;
    const handle = window.setTimeout(
      () => void performMailAction("read", unread, { silent: true }),
      readerFocused ? 600 : 2000,
    );
    return () => window.clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.thread.id, readerFocused]);

  const summarize = useMutation({
    mutationFn: (_input: { silent: boolean }) => summarizeThread(data.thread.id),
    onSuccess: (result, input) => {
      const next = normalizeThreadSummary(result);
      if (next) setSummary(next);
      else if (!input.silent)
        toast.error("Summary failed", { description: "The daemon returned no summary." });
    },
    onError: (error, input) => {
      if (!input.silent) toast.error("Summary failed", { description: error.message });
    },
  });
  const summarizeRef = useRef(summarize.mutate);
  summarizeRef.current = summarize.mutate;
  // Long conversations get an overview automatically (TUI: 200+ words),
  // only when the daemon has a model to ask.
  const llm = useLlmStatus();
  useEffect(() => {
    if (summary || !llm.enabled) return;
    const words = data.bodies.reduce(
      (total, body) => total + (body.reader_text ?? body.text_plain ?? "").split(/\s+/).length,
      0,
    );
    if (words < 200 || data.messages.length < 2) return;
    const handle = window.setTimeout(() => summarizeRef.current({ silent: true }), 400);
    return () => window.clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.thread.id, llm.enabled]);

  const primaryEmail = parseAddress(messages[0]?.sender_detail ?? messages[0]?.sender).email;
  const commitments = useQuery({
    queryKey: ["commitments", data.thread.account_id, primaryEmail],
    queryFn: () =>
      listCommitments({
        accountId: data.thread.account_id,
        email: primaryEmail ?? undefined,
        status: "open",
      }),
    enabled: Boolean(primaryEmail),
    staleTime: 30_000,
  });
  const resolve = useMutation({
    mutationFn: resolveCommitment,
    onSuccess: () => {
      toast.success("Commitment resolved");
      void commitments.refetch();
    },
    onError: (error) => toast.error("Couldn't resolve", { description: error.message }),
  });
  const openCommitments = useMemo(
    () => extractThreadCommitments(commitments.data),
    [commitments.data],
  );

  const siblings = useCallback(() => nav?.threadIds() ?? [], [nav]);
  const position = useMemo(() => {
    const ids = siblings();
    const index = ids.indexOf(data.thread.id);
    return index >= 0 ? { index, total: ids.length } : null;
  }, [data.thread.id, siblings]);

  const step = useCallback(
    (delta: 1 | -1) => {
      const ids = siblings();
      const index = ids.indexOf(data.thread.id);
      const next = index >= 0 ? ids[index + delta] : undefined;
      if (next) nav?.open(next, { focusReader: true });
      return Boolean(next);
    },
    [data.thread.id, nav, siblings],
  );

  // After the conversation leaves the list: open the next one, like the
  // TUI's archive-and-advance, or go back to the list at the end.
  const leave = useCallback(
    (direction: 1 | -1 = 1) => {
      const ids = siblings();
      const index = ids.indexOf(data.thread.id);
      const next = ids[index + direction] ?? ids[index - direction];
      if (index >= 0 && next && next !== data.thread.id) nav?.open(next, { focusReader: true });
      else nav?.close();
    },
    [data.thread.id, nav, siblings],
  );

  const target = () => targetFromThread(data, messages);
  const verbs = createMailVerbs({
    getTarget: target,
    composeSurface: "inline",
    afterLeave: () => leave(1),
  });

  const scrollBy = (amount: number) =>
    scrollRef.current?.scrollBy({ top: amount, behavior: "smooth" });
  const page = () => (scrollRef.current?.clientHeight ?? 600) * 0.85;
  const focusMessage = (index: number) => {
    const clamped = Math.max(0, Math.min(messages.length - 1, index));
    const message = messages[clamped];
    if (!message) return;
    setFocusIndex(clamped);
    setExpanded((current) => new Set(current).add(message.id));
    requestAnimationFrame(() =>
      cardRefs.current.get(message.id)?.scrollIntoView({ block: "nearest", behavior: "smooth" }),
    );
  };
  const setViewAndRemember = (next: ReaderView) => {
    const value = view === next && next !== "formatted" ? "formatted" : next;
    setView(value);
    setDefaultView(value);
  };

  useScopeController("reader", {
    ...verbs,
    scrollDown: () => scrollBy(80),
    scrollUp: () => scrollBy(-80),
    pageDown: () => scrollBy(page()),
    pageUp: () => scrollBy(-page()),
    top: () => scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" }),
    bottom: () =>
      scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" }),
    nextMessage: () => focusMessage(focusIndex + 1),
    prevMessage: () => focusMessage(focusIndex - 1),
    toggleMessage: () => {
      const message = messages[focusIndex];
      if (!message) return;
      setExpanded((current) => {
        const next = new Set(current);
        if (next.has(message.id)) next.delete(message.id);
        else next.add(message.id);
        return next;
      });
    },
    expandAll: () =>
      setExpanded((current) =>
        current.size === messages.length
          ? initialExpanded(data)
          : new Set(messages.map((message) => message.id)),
      ),
    nextThread: () => {
      if (!step(1)) toast.info("That was the last conversation");
    },
    prevThread: () => {
      if (!step(-1)) toast.info("That was the first conversation");
    },
    archiveNext: () => {
      void performMailAction("archive", target().messageIds);
      leave(1);
    },
    archivePrev: () => {
      void performMailAction("archive", target().messageIds);
      leave(-1);
    },
    focusList: () => setActivePane("mailbox"),
    close: () => nav?.close(),
    viewReader: () => setViewAndRemember("reader"),
    viewHtml: () => setViewAndRemember("formatted"),
    viewPlain: () => setViewAndRemember("plain"),
    toggleRemote: () => setRemoteAllowed((value) => !value),
    toggleSignature: () => setShowSignature((value) => !value),
    toggleQuotes: () => setShowQuotes((value) => !value),
    headers: () => {
      const message = messages[focusIndex] ?? messages.at(-1);
      if (message) setHeadersFor(message.id);
    },
    openOriginal: () => {
      const message = messages[focusIndex] ?? messages.at(-1);
      const body = message ? bodies.get(message.id) : undefined;
      const html = body?.text_html;
      const doc = html
        ? standaloneHtmlDocument(html, remoteAllowed)
        : `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="script-src 'none'"><pre style="white-space:pre-wrap;font:14px/1.5 ui-monospace,monospace;padding:24px">${escapeHtml(body?.text_plain ?? body?.reader_text ?? "")}</pre>`;
      const url = URL.createObjectURL(new Blob([doc], { type: "text/html" }));
      window.open(url, "_blank", "noopener,noreferrer");
      setTimeout(() => URL.revokeObjectURL(url), 60_000);
    },
    summarize: () => {
      if (llm.isSuccess && !llm.enabled) {
        toast.info("Summaries need a language model", {
          description: "Add one under Settings, then press y again.",
          action: {
            label: "Set up",
            onClick: () => getRuntimeNavigate().navigate("/settings/llm"),
          },
        });
        return;
      }
      summarize.mutate({ silent: false });
    },
    fullscreen: () => setReaderLayout(readerLayout === "full" ? "split" : "full"),
    draftAssist: () =>
      useModals.getState().openRightRail("draft-assist", { threadId: data.thread.id }),
    ...(nav?.queueLabel
      ? {
          route: () =>
            openMailDialog({
              kind: "move",
              target: target(),
              route: { fromQueueLabel: nav.queueLabel! },
              onDone: () => leave(1),
            }),
        }
      : {}),
  });

  const labels = useMemo(() => {
    const seen = new Map<string, NonNullable<(typeof messages)[number]["labels"]>[number]>();
    for (const message of messages)
      for (const label of message.labels ?? [])
        if (label.kind !== "system") seen.set(label.id, label);
    return [...seen.values()];
  }, [messages]);
  const newest = messages.at(-1);
  const recipients = (newest?.to?.length ?? 0) + (newest?.cc?.length ?? 0);
  const canReplyAll = recipients > 1 || data.thread.participants.length > 2;
  const hasAttachments = data.bodies.some((body) =>
    (body.attachments ?? []).some((attachment) => !attachment.content_id),
  );

  return (
    <article
      aria-label={`Conversation: ${data.thread.subject || "no subject"}`}
      className="@container flex min-h-0 min-w-0 flex-1 flex-col bg-background"
      data-active-pane={readerFocused ? "true" : undefined}
      onMouseDown={() => setActivePane("reader")}
    >
      <ThreadHeader
        data={data}
        labels={labels}
        starred={messages.some((message) => message.starred)}
        canReplyAll={canReplyAll}
        hasAttachments={hasAttachments}
        view={view}
        full={readerLayout === "full"}
        position={position}
      />
      <div
        ref={scrollRef}
        tabIndex={-1}
        data-testid="thread-scroll"
        className={`min-h-0 flex-1 overflow-y-auto outline-none transition-opacity ${stale ? "opacity-60" : ""}`}
        onFocus={() => setActivePane("reader")}
      >
        <div className="mx-auto w-full max-w-[980px] pb-24">
          {summary || summarize.isPending || openCommitments.length > 0 ? (
            <div className="px-5 pt-4">
              {summary ? (
                <ThreadSummaryAccordion
                  summary={summary}
                  expanded
                  onExpandedChange={() => setSummary(null)}
                />
              ) : summarize.isPending ? (
                <ThreadSummaryLoading />
              ) : null}
              {openCommitments.length > 0 ? (
                <ThreadCommitmentChips
                  commitments={openCommitments}
                  resolving={resolve.isPending}
                  onResolve={(id) => resolve.mutate(id)}
                />
              ) : null}
            </div>
          ) : null}
          {messages.map((message, index) => (
            <MessageCard
              key={message.id}
              ref={(node) => {
                if (node) cardRefs.current.set(message.id, node);
                else cardRefs.current.delete(message.id);
              }}
              message={message}
              body={bodies.get(message.id)}
              expanded={expanded.has(message.id)}
              focused={readerFocused && index === focusIndex}
              view={view}
              showQuotes={showQuotes}
              showSignature={showSignature}
              remoteAllowedForThread={remoteAllowed}
              onToggle={() => {
                setFocusIndex(index);
                setExpanded((current) => {
                  const next = new Set(current);
                  if (next.has(message.id)) next.delete(message.id);
                  else next.add(message.id);
                  return next;
                });
              }}
              onAllowRemote={() => setRemoteAllowed(true)}
              onShowHeaders={() => setHeadersFor(message.id)}
            />
          ))}
          {/* ComposeHost portals the inline reply composer here. */}
          <div id="inline-composer-slot" className="px-5 pt-4 empty:hidden" />
        </div>
      </div>
      {headersFor ? (
        <HeadersDialog messageId={headersFor} onClose={() => setHeadersFor(null)} />
      ) : null}
    </article>
  );
}

/** Newest message and every unread one start open; the rest fold. */
function initialExpanded(data: ThreadResponse): Set<string> {
  const open = new Set(
    data.messages.filter((message) => message.unread).map((message) => message.id),
  );
  const newest = data.messages.at(-1);
  if (newest) open.add(newest.id);
  return open;
}

function lastExpandedIndex(data: ThreadResponse, expanded: Set<string>): number {
  const firstUnread = data.messages.findIndex((message) => message.unread);
  if (firstUnread >= 0) return firstUnread;
  let index = 0;
  data.messages.forEach((message, i) => {
    if (expanded.has(message.id)) index = i;
  });
  return index;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => `&#${char.charCodeAt(0)};`);
}

function ReaderSkeleton() {
  return (
    <div
      className="flex min-w-0 flex-1 flex-col"
      aria-busy="true"
      aria-label="Loading conversation"
    >
      <div className="border-b border-border px-5 py-4">
        <div className="h-5 w-2/3 animate-pulse rounded bg-muted" />
        <div className="mt-2 h-3 w-1/3 animate-pulse rounded bg-muted/70" />
      </div>
      {[0, 1].map((index) => (
        <div key={index} className="border-b border-border/70 px-5 py-4">
          <div className="flex items-center gap-3">
            <div className="size-8 animate-pulse rounded-full bg-muted" />
            <div className="h-3 w-40 animate-pulse rounded bg-muted" />
          </div>
          <div className="ml-11 mt-4 grid gap-2">
            <div className="h-3 w-5/6 animate-pulse rounded bg-muted/70" />
            <div className="h-3 w-4/6 animate-pulse rounded bg-muted/70" />
            <div className="h-3 w-3/6 animate-pulse rounded bg-muted/70" />
          </div>
        </div>
      ))}
    </div>
  );
}
