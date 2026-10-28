import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  ArrowLeft,
  Bookmark,
  BookOpenText,
  Check,
  ExternalLink,
  FileText,
  Highlighter,
  Loader2,
  MailX,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Segmented } from "@/components/Segmented";
import { PageFrame } from "@/components/PageFrame";
import { ReaderToolbar, ToolButton } from "@/components/ToolButton";
import { Received } from "@/components/When";
import { Button } from "@/components/ui/button";
import { fetchThread } from "@/features/mailbox/api";
import { BridgeRequestError } from "@/api/client";
import { PageError } from "@/components/PageParts";
import { MessageContent } from "@/features/thread/MessageCard";
import { PrivacyLine } from "@/features/thread/privacy/PrivacyLine";
import { pickSearch, useSearchParam } from "@/hooks/useSearchParam";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { formatLongDate } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { definedShortcut, stripKeys } from "@/lib/actions/definition";
import { usePageKeys } from "@/state/pageKeysStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import {
  READING_KEY,
  recordEngagement,
  useItemQuery,
  useMarginaliaStatus,
  type ReadingDetail,
  type ReadingHighlight,
  type ReadingParagraph,
} from "./api";
import { HighlightToolbar, NoteComposer } from "./HighlightToolbar";
import { ReaderArticle, useReaderHtml } from "./ReaderArticle";
import { PARAGRAPH_ATTRIBUTE, selectionParagraph } from "./paragraphAnchor";
import { ReaderSettingsMenu } from "./ReaderSettingsMenu";
import { readingRootProps, useAppTheme } from "./readerAppearance";
import { ReadingUnsubscribeDialog } from "./ReadingDialogs";
import {
  editionEntries,
  READING_LIST_SEARCH,
  isOwnHighlight,
  marksForParagraph,
  marksForView,
  type AnchoredMark,
  type ParagraphMark,
  minutesLeft,
  scrollProgress,
  visibleBands,
  domainLabel,
} from "./readingView";
import {
  editHighlightNote,
  fetchLinked,
  highlightSelection,
  letGo,
  openInMarginalia,
  putOnLater,
  refreshMarginaliaHighlights,
  rememberLayout,
  removeHighlight,
} from "./readingVerbs";
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
      <PageError
        framed
        title="Couldn't open this item"
        subject="this item"
        error={item.error ?? new BridgeRequestError(404, "Reading item not found")}
        onRetry={() => void item.refetch()}
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
  // Issue or Article is in the URL, so a copied link opens the same one. A
  // link never fetches the article by itself: that tells its site you came.
  const [shown, setShown] = useSearchParam("as");
  const view: "issue" | "article" =
    shown === "issue" || (shown === "article" && hasArticle)
      ? shown
      : item.kind === "link" && detail.article
        ? "article"
        : "issue";
  const setView = setShown;
  const [original, setOriginal] = useState(source.original_layout && view === "issue");
  const [fetching, setFetching] = useState(false);
  const [unsubscribing, setUnsubscribing] = useState(false);
  const [progress, setProgress] = useState(item.progress);
  const columnRef = useRef<HTMLDivElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  // The article's own text: only a selection inside it offers to highlight.
  const bodyRef = useRef<HTMLDivElement>(null);

  const article = detail.article ?? null;
  const showingArticle = view === "article" && article !== null;
  const words = showingArticle ? article.words : item.words;
  const paragraphs = showingArticle ? article.paragraphs : detail.paragraphs;
  // Remote images follow email: held back until asked for, or the sender is trusted.
  const [remoteAllowed, setRemoteAllowed] = useState(false);
  const trustedSenders = useUiPrefs((state) => state.remoteImageSenders);
  const allowSender = useUiPrefs((state) => state.allowRemoteImagesFrom);
  const imagesAllowed =
    remoteAllowed || trustedSenders.includes(item.sender_email.trim().toLowerCase());
  const rawHtml = showingArticle ? article.html : detail.html;
  const articleHtml = useReaderHtml(rawHtml, imagesAllowed);
  const display = useUiPrefs((state) => state.readingDisplay);
  const rootProps = readingRootProps(display, useAppTheme());
  const left = minutesLeft(words, progress, detail.pace_wpm);
  useEngagement(item.item_key, columnRef, setProgress);

  // Back on the list, the cursor is on the item that was open.
  const back = useCallback(
    () =>
      void navigate({
        to: "/reading",
        search: (prev) => ({ ...pickSearch(prev, READING_LIST_SEARCH), row: item.item_key }),
      }),
    [item.item_key, navigate],
  );
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
  }, [article, hasArticle, item.domain, item.item_key, item.tracked, refresh, setView]);

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
      void navigate({
        to: "/reading/item/$itemKey",
        params: { itemKey: following.key },
        search: (prev) => ({ ...pickSearch(prev, READING_LIST_SEARCH), row: following.key }),
      });
    else back();
  }, [back, item.item_key, navigate, qc]);

  const toggleOriginal = useCallback(() => {
    const value = !original;
    setOriginal(value);
    if (value) setView("issue");
    void rememberLayout(source.account_id, source.sender_email, value);
  }, [original, setView, source.account_id, source.sender_email]);

  const highlight = useCallback(() => {
    const selection = window.getSelection()?.toString() ?? "";
    void highlightSelection(
      selection,
      item.item_key,
      showingArticle ? "article" : "issue",
      undefined,
      selectionParagraph(bodyRef.current),
    );
  }, [item.item_key, showingArticle]);

  const marginaliaStatus = useMarginaliaStatus();
  const sentTo = detail.marginalia ?? null;
  const openMarginalia = useCallback(
    () =>
      void openInMarginalia(item.item_key, sentTo?.read_url, {
        connected: marginaliaStatus.data?.connected ?? false,
        onConnect: () =>
          void navigate({ to: "/settings/$section", params: { section: "marginalia" } }),
      }),
    [item.item_key, marginaliaStatus.data?.connected, navigate, sentTo?.read_url],
  );
  // Highlights made in Marginalia come back when a sent item opens.
  const sentArticle = sentTo?.article_id;
  useEffect(() => {
    if (sentArticle) void refreshMarginaliaHighlights(item.item_key);
  }, [item.item_key, sentArticle]);

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
    marginalia: openMarginalia,
    back,
  });

  usePageKeys(stripKeys("reading-reader"));
  const highlights = detail.highlights;
  const shownView = showingArticle ? "article" : "issue";
  const marks = useMemo(() => marksForView(highlights, shownView), [highlights, shownView]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" data-testid="reading-reader">
      <header className="shrink-0">
        <ReaderToolbar wrap width="reading">
          <ToolButton
            icon={ArrowLeft}
            label="Reading"
            keys="Escape"
            hint="Back to Reading"
            onClick={back}
          />
          {hasArticle ? (
            <Segmented
              label="What to read"
              value={view}
              keys={{
                issue: definedShortcut("reading.reader-issue"),
                article: definedShortcut("reading.reader-article"),
              }}
              options={[
                ["issue", "Issue"],
                ["article", "Article"],
              ]}
              onChange={(choice) => (choice === "issue" ? setView("issue") : void openArticle())}
            />
          ) : null}
          {view === "article" && !article ? null : (
            <span data-testid="minutes-left" className="type-meta px-2 text-text-3">
              {left > 0 ? `${left} min left` : "Read to the end"}
            </span>
          )}
          <ReaderSettingsMenu />
          {/* The reading keys keep their words; the rarer verbs are icons named
              in their tooltip, so the bar fits the reading column. */}
          <span className="ml-auto flex flex-wrap items-center gap-1">
            <ToolButton
              label="Later"
              keys="b"
              icon={Bookmark}
              onClick={() =>
                void putOnLater(
                  item.item_key,
                  item.url
                    ? { domain: item.domain, tracked: item.tracked, cached: Boolean(article) }
                    : undefined,
                )
              }
            />
            <ToolButton
              label="Let go"
              keys="e"
              icon={Check}
              onClick={() => {
                void letGo([item.thread_id]);
                next();
              }}
            />
            <ToolButton
              label="Highlight"
              keys={definedShortcut("reading.reader-highlight")}
              icon={Highlighter}
              onClick={highlight}
            />
            <ToolButton
              label={original ? "Cleaned" : "Original"}
              keys="R"
              iconOnly
              icon={FileText}
              hint={original ? "Back to the cleaned layout" : "Show the sender's layout"}
              onClick={toggleOriginal}
            />
            <ToolButton
              testId="reader-marginalia"
              label={sentTo ? "In Marginalia" : "Marginalia"}
              keys={definedShortcut("reading.reader-marginalia")}
              iconOnly
              icon={BookOpenText}
              hint={
                sentTo
                  ? "Open it in Marginalia"
                  : "Send it to Marginalia and open it there: its text goes to Marginalia"
              }
              onClick={openMarginalia}
            />
            <ToolButton
              testId="reader-unsubscribe"
              label="Unsubscribe"
              keys="D"
              iconOnly
              icon={MailX}
              onClick={() => setUnsubscribing(true)}
            />
          </span>
        </ReaderToolbar>
        <PageFrame
          width="reading"
          wrapTitle
          publishTitle={false}
          purposeTestId="reading-source"
          title={showingArticle && article.title ? article.title : item.title}
          purpose={
            showingArticle ? (
              <>
                {[article.site_name ?? domainLabel(item.domain), article.byline]
                  .filter(Boolean)
                  .map((part) => `${part} · `)
                  .join("")}
                <Received value={item.arrived_at} />
              </>
            ) : (
              <>
                {item.source} · <Received value={item.arrived_at} />
              </>
            )
          }
        />
        <div className="h-[2px] w-full bg-border/60" aria-hidden>
          <div
            data-testid="reading-progress"
            className="reading-progress h-full bg-primary"
            style={{ transform: `scaleX(${progress})` }}
          />
        </div>
      </header>
      <div
        ref={columnRef}
        // Both-edges gutter: with a classic scrollbar (WebKit shows one once the issue
        // overflows) the article centres in a narrower box than the toolbar and title
        // above it, and the shared left edge slips by half the scrollbar. Overlay
        // scrollbars take no space, so nothing changes there.
        className="reader-root reading-themed min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable_both-edges]"
        data-testid="reader-column"
        {...rootProps}
      >
        {original && view === "issue" ? (
          <OriginalLayout
            threadId={item.thread_id}
            messageId={item.message_id}
            sender={item.sender_email}
          />
        ) : (
          <article
            ref={textRef}
            // The title above and the text share one column, so the page has a single left edge.
            data-frame="reading"
            className="reading-page mode-frame px-5 pb-16 pt-8"
            // The measure is in ch of the reader's own face and size, so a larger text keeps its line length.
            style={{ fontSize: "var(--reader-size)", fontFamily: "var(--reader-font)" }}
          >
            <div className="max-w-[var(--reading-measure)]">
              {view === "article" ? (
                <ArticleStatus
                  fetching={fetching}
                  detail={detail}
                  onFetch={() => void openArticle()}
                />
              ) : null}
              {view === "article" && !article ? null : (
                <>
                  {articleHtml && rawHtml ? (
                    <PrivacyLine
                      html={rawHtml}
                      imagesAllowed={imagesAllowed}
                      onShowImages={() => setRemoteAllowed(true)}
                      senderEmail={item.sender_email}
                      onAlwaysAllowSender={allowSender}
                      showImagesKey={null}
                    />
                  ) : null}
                  <div ref={bodyRef}>
                    {articleHtml ? (
                      <ReaderArticle
                        html={articleHtml}
                        paragraphs={paragraphs}
                        marks={marks}
                        allowRemoteImages={imagesAllowed}
                      />
                    ) : (
                      <Paragraphs paragraphs={paragraphs} marks={marks} />
                    )}
                  </div>
                  <HighlightToolbar
                    containerRef={bodyRef}
                    itemKey={item.item_key}
                    view={showingArticle ? "article" : "issue"}
                    highlights={highlights}
                    onOpenMarginalia={openMarginalia}
                  />
                </>
              )}
              {view === "issue" && (item.links?.length ?? 0) > 0 ? (
                <IssueLinks detail={detail} />
              ) : null}
              {highlights.length > 0 ? (
                <section aria-label="Your highlights" className="mt-10 border-t border-border pt-4">
                  <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    Your highlights
                  </h2>
                  <ul className="mt-2 grid gap-2" data-testid="reading-highlights">
                    {highlights.map((h) => (
                      <HighlightRow
                        key={h.id}
                        highlight={h}
                        itemKey={item.item_key}
                        onOpenMarginalia={openMarginalia}
                      />
                    ))}
                  </ul>
                </section>
              ) : null}
              <footer
                data-testid="end-of-issue"
                className="mt-12 border-t border-border pt-4 text-sm text-muted-foreground"
              >
                <p>From this source: {source.evidence}.</p>
                {/* Later, Let go and Unsubscribe live in the toolbar above, once. */}
                <p className="-ml-2 mt-2 flex flex-wrap items-center gap-1">
                  <ToolButton label="Next" keys="n" onClick={next} />
                </p>
              </footer>
            </div>
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
        className="mt-6 inline-flex items-center gap-2 text-base text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" /> Fetching from {site}…
      </p>
    );
  }
  if (article) {
    const hosts = article.contacted;
    return (
      <p data-testid="article-status" className="mt-3 text-xs text-muted-foreground">
        {hosts.length > 0 ? `Fetched from ${hosts.join(", ")}` : "Saved copy"} on{" "}
        {formatLongDate(article.fetched_at)}. It reads offline.
      </p>
    );
  }
  if (error) {
    return (
      <div data-testid="article-status" className="mt-6 text-base">
        <p className="text-foreground/90">
          The article couldn't be read here. The email is still here, and the original opens in your
          browser.
        </p>
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
        <details className="mt-3 text-xs text-muted-foreground">
          <summary className="w-fit cursor-pointer underline underline-offset-4">Details</summary>
          <pre className="mt-1 whitespace-pre-wrap break-words font-mono">{error}</pre>
        </details>
      </div>
    );
  }
  return (
    <div data-testid="article-status" className="mt-6 text-base text-foreground/90">
      <p>
        The article is on {site}. Fetching it tells that site you clicked, so mxr only does it when
        you ask.
      </p>
      <Button
        size="sm"
        variant="outline"
        className="mt-3"
        title="Fetch (L)"
        aria-keyshortcuts="L"
        onClick={onFetch}
      >
        Fetch from {site}
      </Button>
    </div>
  );
}

/** One saved highlight: its note, and edit and remove (or where to change it, if made in Marginalia). */
function HighlightRow({
  highlight: h,
  itemKey,
  onOpenMarginalia,
}: {
  highlight: ReadingHighlight;
  itemKey: string;
  onOpenMarginalia: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const own = isOwnHighlight(h);
  return (
    <li
      data-testid="reading-highlight"
      className="border-l-2 border-primary pl-3 text-base text-foreground/90"
    >
      {h.quote}
      {h.note ? <p className="text-xs text-muted-foreground">{h.note}</p> : null}
      {editing ? (
        <div className="mt-1.5 max-w-sm">
          <NoteComposer
            initial={h.note ?? ""}
            onCancel={() => setEditing(false)}
            onSave={async (note) => {
              if (await editHighlightNote(itemKey, h.id, note)) setEditing(false);
            }}
          />
        </div>
      ) : (
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-muted-foreground">
          {own ? (
            <>
              <button
                type="button"
                data-testid="highlight-edit-note"
                onClick={() => setEditing(true)}
                className="hover:text-foreground"
              >
                {h.note ? "Edit note" : "Add note"}
              </button>
              <button
                type="button"
                data-testid="highlight-remove"
                onClick={() => void removeHighlight(itemKey, h.id)}
                className="hover:text-foreground"
              >
                Remove
              </button>
            </>
          ) : (
            <>
              <span>Made in Marginalia; change it there</span>
              <button
                type="button"
                data-testid="highlight-open-marginalia"
                onClick={onOpenMarginalia}
                className="hover:text-foreground"
              >
                Open in Marginalia
              </button>
            </>
          )}
        </div>
      )}
    </li>
  );
}

/** The text, paragraph by paragraph, with your highlights marked. */
export function Paragraphs({
  paragraphs,
  marks,
}: {
  paragraphs: ReadingParagraph[];
  marks: AnchoredMark[];
}) {
  return (
    <div
      className="mt-6 grid gap-4 text-lg leading-[1.65] text-foreground sm:text-lg"
      data-testid="reader-text"
    >
      {paragraphs.map((paragraph, index) => {
        const key = `${index}-${paragraph.text.slice(0, 12)}`;
        // The anchor a highlight made here is saved with and drawn by.
        const tag = { [PARAGRAPH_ATTRIBUTE]: index };
        const content = (
          <Marked text={paragraph.text} marks={marksForParagraph(paragraphs, marks, index)} />
        );
        switch (paragraph.kind) {
          case "heading":
            return (
              <h2 key={key} {...tag} className="mt-4 text-xl font-semibold leading-snug">
                {content}
              </h2>
            );
          case "quote":
            return (
              <blockquote
                key={key}
                {...tag}
                className="border-l-2 border-border pl-4 text-foreground/85"
              >
                {content}
              </blockquote>
            );
          case "list_item":
            return (
              <p key={key} {...tag} className="pl-5 -indent-3">
                <span aria-hidden>• </span>
                {content}
              </p>
            );
          default:
            return (
              <p key={key} {...tag}>
                {content}
              </p>
            );
        }
      })}
    </div>
  );
}

/**
 * Draws each mark in `text`: wherever its words appear, or only the first
 * match for an anchored one. Text only, no markup built.
 */
function Marked({ text, marks }: { text: string; marks: ParagraphMark[] }) {
  const hit = marks
    .map((mark) => ({ mark, at: text.indexOf(mark.quote) }))
    .filter((found) => found.mark.quote.length > 0 && found.at >= 0)
    .toSorted((a, b) => a.at - b.at)[0];
  if (!hit) return <>{text}</>;
  // An anchored mark is spent once drawn; the rest of the paragraph can't show it again.
  const rest = hit.mark.once ? marks.filter((mark) => mark !== hit.mark) : marks;
  return (
    <>
      {text.slice(0, hit.at)}
      <mark
        className="reading-mark cursor-pointer"
        // Selecting it is how the toolbar finds the highlight to edit.
        onClick={(event) => window.getSelection()?.selectAllChildren(event.currentTarget)}
      >
        {hit.mark.quote}
      </mark>
      <Marked text={text.slice(hit.at + hit.mark.quote.length)} marks={rest} />
    </>
  );
}

/** A digest's links under its text, each a reader item of its own. */
function IssueLinks({ detail }: { detail: ReadingDetail }) {
  const navigate = useNavigate();
  const links = detail.item.links ?? [];
  return (
    <section aria-label="Links in this issue" className="mt-10 border-t border-border pt-4">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        Links in this issue
      </h2>
      <ul className="mt-2 grid gap-1.5">
        {links.map((link) => (
          <li key={link.item_key} className="text-md">
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
      <p className="p-6 text-sm text-muted-foreground">
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
