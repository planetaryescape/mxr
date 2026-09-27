import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { performMailAction } from "@/features/mail-actions/mailMutations";
import { createMailVerbs } from "@/features/mail-actions/mailVerbs";
import { llmPolicyKey, useLlmStatus } from "@/features/llm/useLlmStatus";
import { useProjectedMessages } from "@/features/mail-actions/pendingMailOps";
import { targetFromThread } from "@/features/mail-actions/target";
import { resolveCommitment } from "@/features/mailbox/api";
import { useReaderNav } from "@/features/mailbox/readerNav";
import type { ThreadResponse } from "@/features/mailbox/types";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useUiPrefs, type ReaderView } from "@/state/uiPrefsStore";

import {
  fetchThreadGist,
  threadContextKey,
  threadContextQuery,
  threadGistKey,
  threadGistQuery,
} from "./context/api";
import { ASK_MARK_ATTRIBUTE } from "./context/askQuote";
import { ContextBlock } from "./context/ContextBlock";
import { firstName } from "./context/contextFormat";
import { HeadersDialog } from "./HeadersDialog";
import { standaloneHtmlDocument } from "./MessageBody";
import { MessageCard } from "./MessageCard";
import { initialExpanded, lastExpandedIndex } from "./threadExpansion";
import { ThreadHeader } from "./ThreadHeader";
import { ReplyField } from "./ReplyField";
import { ThreadSummaryAccordion, ThreadSummaryLoading } from "./ThreadInsights";
import { useThreadSiblings } from "./useThreadSiblings";
import { useThreadSummary } from "./useThreadSummary";

/** How much of the landing message must show before the reader scrolls to it. */
const LANDING_MARGIN_PX = 160;

/** One loaded conversation: context, messages, the reply field and reader keys. */
export function ThreadReader({ data }: { data: ThreadResponse }) {
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
  const { summary, setSummary, summarize } = useThreadSummary(data);
  const llm = useLlmStatus();

  const readerFocused = activePane === "reader";
  const singlePane = useMediaQuery(SINGLE_PANE_QUERY);
  const listHidden = singlePane || readerLayout === "full";
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

  // Land on the newest unread message, like a mail client, but only scroll
  // when it starts below the fold: otherwise the top of the thread, with its
  // context block, stays in view.
  useEffect(() => {
    const target = messages[focusIndex];
    const node = target ? cardRefs.current.get(target.id) : undefined;
    const container = scrollRef.current;
    if (!node || !container || focusIndex === 0) return;
    const foldTop = container.getBoundingClientRect().bottom - LANDING_MARGIN_PX;
    if (node.getBoundingClientRect().top > foldTop) node.scrollIntoView({ block: "start" });
    // Only on first render of this thread.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Mark read after a short dwell: at once when the reader has focus, after
  // two seconds when the list is previewing threads under the cursor. Once
  // per opening: after it fires, or after the user marks read or unread
  // themselves, pane changes never mark the thread read again.
  const autoReadSettled = useRef(false);
  useEffect(() => {
    if (autoReadSettled.current) return;
    const unread = messages.filter((message) => message.unread).map((message) => message.id);
    if (unread.length === 0) return;
    const handle = window.setTimeout(
      () => {
        autoReadSettled.current = true;
        void performMailAction("read", unread, { silent: true });
      },
      readerFocused ? 600 : 2000,
    );
    return () => window.clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.thread.id, readerFocused]);

  const threadId = data.thread.id;
  const queryClient = useQueryClient();
  const context = useQuery(threadContextQuery(threadId));
  const policy = llmPolicyKey(llm.data?.status);
  const gist = useQuery({ ...threadGistQuery(threadId, policy), enabled: llm.enabled });
  const resolve = useMutation({
    mutationFn: resolveCommitment,
    onSuccess: () => {
      toast.success("Promise marked done");
      void queryClient.invalidateQueries({ queryKey: threadContextKey(threadId) });
    },
    onError: (error) => toast.error("Couldn't mark it done", { description: error.message }),
  });
  const askQuote = gist.data?.status === "ready" ? (gist.data.ask?.quote ?? null) : null;

  // "Show in message": expand the message, then scroll to its mark. The
  // formatted view marks inside the frame when the quote's text survives
  // sanitizing; when it doesn't, fall back to the reader view.
  const pendingReveal = useRef(false);
  const scrollToAskMark = (): boolean => {
    if (!askQuote) return false;
    const card = cardRefs.current.get(askQuote.message_id);
    const container = scrollRef.current;
    if (!card || !container) return false;
    const selector = `[${ASK_MARK_ATTRIBUTE}]`;
    const inPage = card.querySelector<HTMLElement>(selector);
    if (inPage) {
      inPage.scrollIntoView({ block: "center", behavior: "smooth" });
      return true;
    }
    const frame = card.querySelector("iframe");
    const inFrame = frame?.contentDocument?.querySelector<HTMLElement>(selector);
    if (!frame || !inFrame) return false;
    const top =
      frame.getBoundingClientRect().top +
      inFrame.getBoundingClientRect().top -
      container.getBoundingClientRect().top;
    container.scrollBy({ top: top - container.clientHeight / 3, behavior: "smooth" });
    return true;
  };
  const revealAsk = () => {
    if (!askQuote) return;
    const index = messages.findIndex((message) => message.id === askQuote.message_id);
    if (index < 0) return;
    setFocusIndex(index);
    setExpanded((current) => new Set(current).add(askQuote.message_id));
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (scrollToAskMark() || view === "reader") return;
        pendingReveal.current = true;
        setView("reader");
      }),
    );
  };
  useEffect(() => {
    if (!pendingReveal.current) return;
    pendingReveal.current = false;
    requestAnimationFrame(() => scrollToAskMark());
    // Only after the view switch that revealAsk asked for.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);
  const { position, step, leave } = useThreadSiblings(data.thread.id, nav);

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
    markRead: () => {
      autoReadSettled.current = true;
      verbs.markRead?.();
    },
    markUnread: () => {
      autoReadSettled.current = true;
      verbs.markUnread?.();
    },
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
    // With the list hidden (narrow screen, full-width reader), going back to
    // it means closing the conversation, not focusing an invisible list.
    focusList: () => (listHidden ? nav?.close() : setActivePane("mailbox")),
    close: () => nav?.close(),
    // Toggles, as in the TUI: R or H again drops back to plain text.
    viewReader: () => setViewAndRemember(view === "reader" ? "plain" : "reader"),
    viewHtml: () => setViewAndRemember(view === "formatted" ? "plain" : "formatted"),
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
      summarize.mutate();
    },
    fullscreen: () => setReaderLayout(readerLayout === "full" ? "split" : "full"),
    draftAssist: () =>
      useModals.getState().openRightRail("draft-assist", {
        threadId: data.thread.id,
        messageId: target().primary?.id,
      }),
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
        className="min-h-0 flex-1 overflow-y-auto outline-none"
        onFocus={() => setActivePane("reader")}
      >
        <div className="mx-auto w-full max-w-[980px] pb-24">
          <ContextBlock
            context={context.data}
            gist={{
              reserved: llm.enabled,
              data: gist.data,
              loading: gist.isFetching && !gist.data,
              error: gist.error?.message ?? null,
              retry: () =>
                void queryClient.fetchQuery({
                  queryKey: threadGistKey(threadId, policy),
                  queryFn: () => fetchThreadGist(threadId, true),
                  staleTime: 0,
                }),
            }}
            onRevealAsk={revealAsk}
            onResolvePromise={(id) => resolve.mutate(id)}
            resolving={resolve.isPending}
          />
          {summary || summarize.isPending ? (
            <div className="px-5 pt-4">
              {summary ? (
                <ThreadSummaryAccordion
                  summary={summary}
                  expanded
                  onExpandedChange={() => setSummary(null)}
                />
              ) : (
                <ThreadSummaryLoading />
              )}
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
              askQuote={askQuote?.message_id === message.id ? askQuote.text : undefined}
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
              onShowFormatted={() => setView("formatted")}
              onShowHeaders={() => setHeadersFor(message.id)}
            />
          ))}
          <ReplyField
            name={context.data?.counterparty ? firstName(context.data.counterparty) : null}
            canReplyAll={canReplyAll}
            onDraftInVoice={
              llm.enabled
                ? () => {
                    const primary = target().primary;
                    if (!primary) return;
                    useComposeUi
                      .getState()
                      .openCompose(
                        { ...replyIntent(primary.id, "single"), openAssist: true },
                        "inline",
                      );
                  }
                : undefined
            }
          />
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

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => `&#${char.charCodeAt(0)};`);
}
