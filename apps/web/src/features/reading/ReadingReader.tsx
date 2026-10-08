import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ArrowLeft, ExternalLink, Loader2, MailX, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { fetchThread } from "@/features/mailbox/api";
import { Centered } from "@/features/mailbox/MailViewParts";
import { MessageContent } from "@/features/thread/MessageCard";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { formatLongDate } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";

import {
  READING_KEY,
  recordEngagement,
  useItemQuery,
  type ReadingDetail,
  type ReadingParagraph,
} from "./api";
import { ReadingUnsubscribeDialog } from "./ReadingDialogs";
import {
  editionEntries,
  minutesLeft,
  scrollProgress,
  visibleBands,
  domainLabel,
} from "./readingView";
import { fetchLinked, highlightSelection, letGo, putOnLater, rememberLayout } from "./readingVerbs";
import type { ReadingEdition } from "./api";

/** How often reading time is reported while the page is visible. */
const REPORT_EVERY_MS = 10_000;
/** A page that fits on screen is read after this long on screen. */
const FITS_READ_AFTER_MS = 8_000;

/**
 * The reader: one item in a book-like column about 66 characters wide,
 * with time left at your pace and a thin progress line. The issue is the
 * email's own text, cleaned; Article is the linked page, fetched only when
 * you ask, naming the site first. R shows the sender's layout instead,
 * remembered per source.
 */
export function ReadingReader({ itemKey }: { itemKey: string }) {
  const item = useItemQuery(itemKey);
  if (item.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-muted-foreground">
        <Loader2 className="size-5 animate-spin" aria-label="Loading" />
      </div>
    );
  }
  if (item.isError || !item.data) {
    return (
      <Centered
        icon={<RefreshCw className="size-6" />}
        title="Couldn't open this item"
        body={item.error?.message ?? "It may have been deleted."}
        action={
          <Button size="sm" onClick={() => void item.refetch()}>
            Try again
          </Button>
        }
      />
    );
  }
  return <Reader key={itemKey} detail={item.data} />;
}

function Reader({ detail }: { detail: ReadingDetail }) {
  const { item, source_data: source } = detail;
  const navigate = useNavigate();
  const qc = useQueryClient();
  const hasArticle = Boolean(item.url);
  const [view, setView] = useState<"issue" | "article">(
    item.kind === "link" && detail.article ? "article" : "issue",
  );
  const [original, setOriginal] = useState(source.original_layout && view === "issue");
  const [fetching, setFetching] = useState(false);
  const [unsubscribing, setUnsubscribing] = useState(false);
  const [progress, setProgress] = useState(item.progress);
  const columnRef = useRef<HTMLDivElement>(null);
  const textRef = useRef<HTMLDivElement>(null);

  const article = detail.article ?? null;
  const showingArticle = view === "article" && article !== null;
  const words = showingArticle ? article.words : item.words;
  const paragraphs = showingArticle ? article.paragraphs : detail.paragraphs;
  const left = minutesLeft(words, progress, detail.pace_wpm);
  useEngagement(item.item_key, columnRef, setProgress);

  const back = useCallback(() => void navigate({ to: "/reading" }), [navigate]);
  const refresh = useCallback(
    () => qc.invalidateQueries({ queryKey: [...READING_KEY, "item", item.item_key] }),
    [qc, item.item_key],
  );

  const openArticle = useCallback(async () => {
    if (!hasArticle) return;
    setView("article");
    setOriginal(false);
    if (article) return;
    setFetching(true);
    await fetchLinked(item.item_key, item.domain, item.tracked);
    await refresh();
    setFetching(false);
  }, [article, hasArticle, item.domain, item.item_key, item.tracked, refresh]);

  const next = useCallback(() => {
    const edition = qc
      .getQueriesData<ReadingEdition>({ queryKey: [...READING_KEY, "edition"] })
      .map(([, data]) => data)
      .find(Boolean);
    if (!edition) return back();
    const entries = editionEntries(visibleBands(edition, new Set()), new Set());
    const at = entries.findIndex((entry) => entry.key === item.item_key);
    const following = entries[at + 1];
    if (following)
      void navigate({ to: "/reading/item/$itemKey", params: { itemKey: following.key } });
    else back();
  }, [back, item.item_key, navigate, qc]);

  const toggleOriginal = useCallback(() => {
    const value = !original;
    setOriginal(value);
    if (value) setView("issue");
    void rememberLayout(source.account_id, source.sender_email, value);
  }, [original, source.account_id, source.sender_email]);

  const highlight = useCallback(() => {
    const selection = window.getSelection()?.toString() ?? "";
    void highlightSelection(selection, item.item_key, showingArticle ? "article" : "issue");
  }, [item.item_key, showingArticle]);

  useShortcutScope("reading-reader");
  useScopeController("reading-reader", {
    down: () => columnRef.current?.scrollBy({ top: 80 }),
    up: () => columnRef.current?.scrollBy({ top: -80 }),
    article: () => void openArticle(),
    issue: () => {
      setView("issue");
    },
    later: () =>
      void putOnLater(
        item.item_key,
        item.url
          ? { domain: item.domain, tracked: item.tracked, cached: Boolean(article) }
          : undefined,
      ),
    letGo: () => {
      void letGo([item.thread_id]);
      next();
    },
    next,
    original: toggleOriginal,
    highlight,
    unsubscribe: () => setUnsubscribing(true),
    openEmail: () =>
      void navigate({ to: "/reading/$threadId", params: { threadId: item.thread_id } }),
    back,
  });

  const highlights = detail.highlights;
  const marks = useMemo(() => highlights.map((h) => h.quote), [highlights]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" data-testid="reading-reader">
      <header className="shrink-0 border-b border-border">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2 text-[12.5px] text-muted-foreground sm:px-5">
          <button
            type="button"
            onClick={back}
            className="inline-flex items-center gap-1 hover:text-foreground"
            aria-label="Back to Reading (Esc)"
          >
            <ArrowLeft className="size-3.5" /> Reading
          </button>
          {hasArticle ? (
            <div
              role="radiogroup"
              aria-label="What to read"
              className="inline-flex rounded-md border border-border"
            >
              <button
                type="button"
                role="radio"
                aria-checked={view === "issue"}
                onClick={() => setView("issue")}
                className={cn("px-2 py-0.5", view === "issue" && "bg-accent text-foreground")}
              >
                Issue
              </button>
              <button
                type="button"
                role="radio"
                aria-checked={view === "article"}
                onClick={() => void openArticle()}
                className={cn("px-2 py-0.5", view === "article" && "bg-accent text-foreground")}
              >
                Article
              </button>
            </div>
          ) : null}
          {view === "article" && !article ? null : (
            <span data-testid="minutes-left" className="tabular-nums">
              {left > 0 ? `${left} min left` : "Read to the end"}
            </span>
          )}
          <button
            type="button"
            data-testid="reader-unsubscribe"
            onClick={() => setUnsubscribing(true)}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-2 py-0.5 hover:border-primary/60 hover:bg-accent hover:text-foreground"
          >
            <MailX aria-hidden className="size-3.5" /> Unsubscribe <KeyChip>D</KeyChip>
          </button>
          <span className="ml-auto hidden items-center gap-3 sm:inline-flex">
            <span className="inline-flex items-center gap-1">
              <KeyChip>b</KeyChip> later
            </span>
            <span className="inline-flex items-center gap-1">
              <KeyChip>e</KeyChip> let go
            </span>
            <span className="inline-flex items-center gap-1">
              <KeyChip>R</KeyChip> {original ? "cleaned" : "original"}
            </span>
            <span className="inline-flex items-center gap-1">
              <KeyChip>h</KeyChip> highlight
            </span>
          </span>
        </div>
        <div className="h-[2px] w-full bg-border/60" aria-hidden>
          <div
            data-testid="reading-progress"
            className="reading-progress h-full bg-primary"
            style={{ transform: `scaleX(${progress})` }}
          />
        </div>
      </header>
      <div ref={columnRef} className="min-h-0 flex-1 overflow-y-auto" data-testid="reader-column">
        {original && view === "issue" ? (
          <OriginalLayout
            threadId={item.thread_id}
            messageId={item.message_id}
            sender={item.sender_email}
          />
        ) : (
          <article
            ref={textRef}
            className="reading-page mx-auto w-full max-w-[66ch] px-4 pb-16 pt-8 sm:px-6"
          >
            <h1 className="text-balance text-[24px] font-semibold leading-tight tracking-tight sm:text-[28px]">
              {showingArticle && article.title ? article.title : item.title}
            </h1>
            <p className="mt-2 text-[13px] text-muted-foreground">
              {showingArticle
                ? [article.site_name ?? domainLabel(item.domain), article.byline]
                    .filter(Boolean)
                    .join(" · ")
                : `${item.source} · ${formatLongDate(item.arrived_at)}`}
            </p>
            {view === "article" ? (
              <ArticleStatus
                fetching={fetching}
                detail={detail}
                onFetch={() => void openArticle()}
              />
            ) : null}
            {view === "article" && !article ? null : (
              <Paragraphs paragraphs={paragraphs} marks={marks} />
            )}
            {view === "issue" && (item.links?.length ?? 0) > 0 ? (
              <IssueLinks detail={detail} />
            ) : null}
            {highlights.length > 0 ? (
              <section aria-label="Your highlights" className="mt-10 border-t border-border pt-4">
                <h2 className="text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Your highlights
                </h2>
                <ul className="mt-2 grid gap-2" data-testid="reading-highlights">
                  {highlights.map((h) => (
                    <li
                      key={h.id}
                      className="border-l-2 border-primary pl-3 text-[14px] text-foreground/90"
                    >
                      {h.quote}
                      {h.note ? (
                        <p className="text-[12.5px] text-muted-foreground">{h.note}</p>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </section>
            ) : null}
            <footer
              data-testid="end-of-issue"
              className="mt-12 border-t border-border pt-4 text-[13px] text-muted-foreground"
            >
              <p>From this source: {source.evidence}.</p>
              <p className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
                <button
                  type="button"
                  onClick={next}
                  className="inline-flex items-center gap-1 hover:text-foreground"
                >
                  <KeyChip>n</KeyChip> next
                </button>
                <button
                  type="button"
                  onClick={() => void putOnLater(item.item_key)}
                  className="inline-flex items-center gap-1 hover:text-foreground"
                >
                  <KeyChip>b</KeyChip> later
                </button>
                <button
                  type="button"
                  onClick={() => {
                    void letGo([item.thread_id]);
                    next();
                  }}
                  className="inline-flex items-center gap-1 hover:text-foreground"
                >
                  <KeyChip>e</KeyChip> let go
                </button>
                <button
                  type="button"
                  onClick={() => setUnsubscribing(true)}
                  className="inline-flex items-center gap-1 hover:text-foreground"
                >
                  <KeyChip>D</KeyChip> unsubscribe
                </button>
              </p>
            </footer>
          </article>
        )}
      </div>
      {unsubscribing ? (
        <ReadingUnsubscribeDialog source={source} onClose={() => setUnsubscribing(false)} />
      ) : null}
    </div>
  );
}

/** "Fetching from sqlite.org…", the failure with the browser, or where it came from. */
function ArticleStatus({
  fetching,
  detail,
  onFetch,
}: {
  fetching: boolean;
  detail: ReadingDetail;
  onFetch: () => void;
}) {
  const { item, article, article_error: error } = detail;
  const site = domainLabel(item.domain, item.tracked) || "the article's site";
  if (fetching) {
    return (
      <p
        data-testid="article-status"
        className="mt-6 inline-flex items-center gap-2 text-[14px] text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" /> Fetching from {site}…
      </p>
    );
  }
  if (article) {
    const hosts = article.contacted;
    return (
      <p data-testid="article-status" className="mt-3 text-[12.5px] text-muted-foreground">
        {hosts.length > 0 ? `Fetched from ${hosts.join(", ")}` : "Saved copy"} on{" "}
        {formatLongDate(article.fetched_at)}. It reads offline.
      </p>
    );
  }
  if (error) {
    return (
      <div data-testid="article-status" className="mt-6 text-[14px]">
        <p className="text-foreground/90">The article couldn't be read here: {error}.</p>
        {item.url ? (
          <a
            href={item.url}
            target="_blank"
            rel="noopener noreferrer"
            className="mt-2 inline-flex items-center gap-1 underline decoration-border-strong underline-offset-4 hover:decoration-primary"
          >
            <ExternalLink className="size-3.5" /> Open in browser
          </a>
        ) : null}
      </div>
    );
  }
  return (
    <div data-testid="article-status" className="mt-6 text-[14px] text-foreground/90">
      <p>
        The article is on {site}. Fetching it tells that site you clicked, so mxr only does it when
        you ask.
      </p>
      <Button size="sm" variant="outline" className="mt-3" onClick={onFetch}>
        Fetch from {site} <KeyChip className="ml-1 h-4 px-1">L</KeyChip>
      </Button>
    </div>
  );
}

/** The text, paragraph by paragraph, with your highlights marked. */
function Paragraphs({ paragraphs, marks }: { paragraphs: ReadingParagraph[]; marks: string[] }) {
  return (
    <div
      className="mt-6 grid gap-4 text-[17px] leading-[1.65] text-foreground sm:text-[18px]"
      data-testid="reader-text"
    >
      {paragraphs.map((paragraph, index) => {
        const key = `${index}-${paragraph.text.slice(0, 12)}`;
        const content = <Marked text={paragraph.text} marks={marks} />;
        switch (paragraph.kind) {
          case "heading":
            return (
              <h2 key={key} className="mt-4 text-[19px] font-semibold leading-snug">
                {content}
              </h2>
            );
          case "quote":
            return (
              <blockquote key={key} className="border-l-2 border-border pl-4 text-foreground/85">
                {content}
              </blockquote>
            );
          case "list_item":
            return (
              <p key={key} className="pl-5 -indent-3">
                <span aria-hidden>• </span>
                {content}
              </p>
            );
          default:
            return <p key={key}>{content}</p>;
        }
      })}
    </div>
  );
}

/** Marks the first place each highlight appears in `text`; text only, no markup built. */
function Marked({ text, marks }: { text: string; marks: string[] }) {
  const hit = marks
    .map((mark) => ({ mark, at: text.indexOf(mark) }))
    .filter((found) => found.mark.length > 0 && found.at >= 0)
    .toSorted((a, b) => a.at - b.at)[0];
  if (!hit) return <>{text}</>;
  return (
    <>
      {text.slice(0, hit.at)}
      <mark className="reading-mark">{hit.mark}</mark>
      <Marked text={text.slice(hit.at + hit.mark.length)} marks={marks} />
    </>
  );
}

/** A digest's links under its text, each a reader item of its own. */
function IssueLinks({ detail }: { detail: ReadingDetail }) {
  const navigate = useNavigate();
  const links = detail.item.links ?? [];
  return (
    <section aria-label="Links in this issue" className="mt-10 border-t border-border pt-4">
      <h2 className="text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
        Links in this issue
      </h2>
      <ul className="mt-2 grid gap-1.5">
        {links.map((link) => (
          <li key={link.item_key} className="text-[15px]">
            <button
              type="button"
              onClick={() =>
                void navigate({ to: "/reading/item/$itemKey", params: { itemKey: link.item_key } })
              }
              className="text-left hover:underline hover:underline-offset-4"
            >
              {link.title}
            </button>{" "}
            <span className="font-mono text-2xs text-muted-foreground">
              {domainLabel(link.domain, link.tracked)}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The sender's own layout: the email's HTML through the reader's sandboxed frame. */
function OriginalLayout({
  threadId,
  messageId,
  sender,
}: {
  threadId: string;
  messageId: string;
  sender: string;
}) {
  const [remoteAllowed, setRemoteAllowed] = useState(false);
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    staleTime: 60_000,
  });
  const body = thread.data?.bodies.find((candidate) => candidate.message_id === messageId);
  const row = thread.data?.messages.find((candidate) => candidate.id === messageId);
  if (thread.isError) {
    return (
      <p className="p-6 text-[13px] text-muted-foreground">
        Couldn't load the original: {thread.error.message}
      </p>
    );
  }
  if (!row) {
    return (
      <div className="flex justify-center p-8 text-muted-foreground">
        <Loader2 className="size-5 animate-spin" aria-label="Loading the original" />
      </div>
    );
  }
  return (
    <div className="mx-auto w-full max-w-[52rem] px-4 py-6" data-testid="original-layout">
      <MessageContent
        message={row}
        body={body}
        view="formatted"
        showQuotes={false}
        showSignature={false}
        remoteAllowed={remoteAllowed}
        onAllowRemote={() => setRemoteAllowed(true)}
        onShowFormatted={() => undefined}
        senderEmail={sender}
      />
    </div>
  );
}

/**
 * Local engagement: opened once on arrival, then time read and how far,
 * every ten seconds while the page is visible and once on leaving. With
 * MXR_ACTIVITY=off the daemon records nothing.
 */
function useEngagement(
  itemKey: string,
  column: React.RefObject<HTMLDivElement | null>,
  onProgress: (progress: number) => void,
) {
  const furthest = useRef(0);
  const since = useRef(Date.now());
  useEffect(() => {
    const node = column.current;
    // A page that fits on screen counts as read only once it has been on
    // screen for a while: opening it is not reading it.
    const fits = () => Boolean(node && node.scrollHeight <= node.clientHeight);
    const measure = () => {
      if (!node || fits()) return;
      const value = scrollProgress(node.scrollTop, node.scrollHeight, node.clientHeight);
      if (value > furthest.current) {
        furthest.current = value;
        onProgress(value);
      }
    };
    let readMs = 0;
    measure();
    void recordEngagement({ itemKey, opened: true, progress: furthest.current }).catch(
      () => undefined,
    );
    since.current = Date.now();
    const report = () => {
      const now = Date.now();
      const visible = document.visibilityState === "visible";
      const dwell = visible ? now - since.current : 0;
      since.current = now;
      readMs += dwell;
      if (fits() && readMs >= FITS_READ_AFTER_MS && furthest.current < 1) {
        furthest.current = 1;
        onProgress(1);
      }
      if (dwell <= 0 && furthest.current <= 0) return;
      void recordEngagement({ itemKey, dwellMs: dwell, progress: furthest.current }).catch(
        () => undefined,
      );
    };
    const onVisibility = () => {
      if (document.visibilityState === "hidden") report();
      else since.current = Date.now();
    };
    node?.addEventListener("scroll", measure, { passive: true });
    document.addEventListener("visibilitychange", onVisibility);
    const timer = window.setInterval(report, REPORT_EVERY_MS);
    return () => {
      window.clearInterval(timer);
      node?.removeEventListener("scroll", measure);
      document.removeEventListener("visibilitychange", onVisibility);
      report();
    };
  }, [column, itemKey, onProgress]);
}
