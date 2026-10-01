import { useQuery } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { ArrowUpRight, MailX, Newspaper, Pin, RefreshCw } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { fetchThread } from "@/features/mailbox/api";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { MessageContent } from "@/features/thread/MessageCard";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { notePointerUse } from "@/lib/actions/keyHints";
import { formatLongDate, formatWhen, plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import type { PlaceBundle } from "./api";
import { PlaceHeader, PlaceLayout } from "./PlaceLayout";
import { bundleKey, bundleSender, readingIssues, whyHere, type ReadingIssue } from "./placeCopy";
import { messagesLeft } from "./placePaging";
import {
  openMoveSender,
  openSweep,
  openUnsubscribe,
  placeMessageRow,
  togglePin,
} from "./placeVerbs";
import { SweptClear } from "./SweptClear";
import { usePlace } from "./usePlace";

/** Issues fetched per sender; the feed shows the newest across senders. */
const ISSUES_PER_SENDER = 20;
/** Issues rendered before "Show more". Each one is an open message. */
const FEED_PAGE = 12;

/**
 * Reading: newsletters and lists as a calm feed. Every issue is already
 * open, newest first; nothing is bold and nothing is counted as unread.
 * j and k move issue to issue.
 */
export function ReadingRoute() {
  const place = usePlace("reading", ISSUES_PER_SENDER);
  const { status } = place;
  const issues = useMemo(() => readingIssues(place.bundles), [place.bundles]);
  const threadIds = useCallback(
    () => [...new Set(issues.map((issue) => issue.message.thread_id))],
    [issues],
  );
  const senders = place.totalBundles;
  const phase = useDelayedPending(status.isLoading);
  const lowTide = useLowTide("reading", status.isLoading || status.isError, issues.length > 0);
  return (
    <PlaceLayout basePath="/reading" label="Reading" threadIds={threadIds} wideReader>
      <PlaceHeader
        title="Reading"
        meta={
          senders > 0
            ? `Newsletters and lists from ${plural(senders, "sender")}, newest first. Nothing here is waiting on you.`
            : "Newsletters and lists, newest first."
        }
        actions={
          issues.length > 0 ? (
            <Button
              variant="outline"
              size="xs"
              onClick={() => {
                openSweep("reading", place.accountId);
                notePointerUse("place.sweep-all");
              }}
            >
              Sweep all <KeyChip className="ml-1 h-4 px-1">A</KeyChip>
            </Button>
          ) : null
        }
      />
      {phase !== "ready" ? (
        <ListSkeleton quiet={phase === "quiet"} />
      ) : status.isError ? (
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't load Reading"
          body={status.error.message}
          action={
            <Button size="sm" onClick={() => void status.refetch()}>
              Try again
            </Button>
          }
        />
      ) : issues.length === 0 && lowTide ? (
        <SweptClear place="Reading" />
      ) : issues.length === 0 ? (
        <Centered
          icon={<Newspaper className="size-6" />}
          title="Nothing to read"
          body="Newsletters and lists that reach your inbox land here, already open."
        />
      ) : (
        <Feed issues={issues} place={place} />
      )}
    </PlaceLayout>
  );
}

/** Unpinned issues on screen, from one sender or all of them. */
function shownUnpinned(visible: readonly ReadingIssue[], bundle?: PlaceBundle): number {
  const key = bundle ? bundleKey(bundle) : null;
  return visible.filter(
    (issue) => !issue.message.pinned && (key === null || bundleKey(issue.bundle) === key),
  ).length;
}

function Feed({ issues, place }: { issues: ReadingIssue[]; place: ReturnType<typeof usePlace> }) {
  const account = useUiPrefs((s) => s.accountScope);
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const [shown, setShown] = useState(FEED_PAGE);
  const [cursorId, setCursorId] = useState<string | null>(null);
  const visible = issues.slice(0, shown);
  const index = Math.max(
    0,
    visible.findIndex((issue) => issue.message.message_id === cursorId),
  );
  const current = visible[index];
  const feedRef = useRef<HTMLDivElement>(null);
  const moved = useRef(false);

  useEffect(() => {
    if (!moved.current) return;
    feedRef.current
      ?.querySelector(`[data-index="${index}"]`)
      ?.scrollIntoView({ block: "start", behavior: "auto" });
  }, [index]);

  // Older issues: the next page of each sender's list and of senders.
  const olderSenders = place.bundles.filter((bundle) => messagesLeft(bundle) > 0);
  const canLoadOlder = olderSenders.length > 0 || place.hasMoreSenders;
  const loadingOlder = place.loadingMoreSenders || place.loadingFrom.size > 0;
  const loadOlder = () => {
    if (loadingOlder) return;
    if (olderSenders.length > 0) place.moreFrom(olderSenders);
    if (place.hasMoreSenders) place.loadMoreSenders();
    setShown((value) => value + FEED_PAGE);
  };

  const move = (delta: number) => {
    const next = Math.min(visible.length - 1, Math.max(0, index + delta));
    if (delta > 0 && next === visible.length - 1) {
      if (shown < issues.length) setShown((value) => value + FEED_PAGE);
      else if (canLoadOlder) loadOlder();
    }
    moved.current = true;
    setCursorId(visible[next]?.message.message_id ?? null);
  };

  useShortcutScope("place", !params.threadId || activePane !== "reader");
  useScopeController("place", {
    down: () => move(1),
    up: () => move(-1),
    open: () => current && nav?.open(current.message.thread_id),
    pin: () => current && void togglePin(current.message),
    sweepBundle: () =>
      current &&
      openSweep("reading", account, current.bundle, shownUnpinned(visible, current.bundle)),
    sweepAll: () => openSweep("reading", account, undefined, shownUnpinned(visible)),
    moveSender: () => current && openMoveSender(current.bundle),
    unsubscribe: () => current && openUnsubscribe(current.bundle, current.message),
  });

  return (
    <div ref={feedRef} className="min-h-0 flex-1 overflow-y-auto" data-testid="reading-feed">
      {visible.map((issue, position) => (
        <Issue
          key={issue.message.message_id}
          index={position}
          issue={issue}
          focused={position === index}
          onFocus={() => setCursorId(issue.message.message_id)}
          onOpen={() => nav?.open(issue.message.thread_id)}
        />
      ))}
      {shown < issues.length ? (
        <div className="px-5 py-6 text-center">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShown((value) => value + FEED_PAGE)}
          >
            Show {Math.min(FEED_PAGE, issues.length - shown)} more
          </Button>
        </div>
      ) : canLoadOlder ? (
        <div className="px-5 py-6 text-center">
          <Button variant="outline" size="sm" disabled={loadingOlder} onClick={loadOlder}>
            {loadingOlder ? "Loading…" : "Show older issues"}
          </Button>
        </div>
      ) : (
        <p className="px-5 py-8 text-center text-[12.5px] text-muted-foreground">
          That is everything in Reading. <KeyChip>A</KeyChip> sweeps what you have read.
        </p>
      )}
    </div>
  );
}

const Issue = memo(function Issue({
  index,
  issue,
  focused,
  onFocus,
  onOpen,
}: {
  index: number;
  issue: ReadingIssue;
  focused: boolean;
  onFocus: () => void;
  onOpen: () => void;
}) {
  const { bundle, message } = issue;
  const sender = bundleSender(bundle);
  return (
    <article
      data-index={index}
      data-testid="reading-issue"
      data-sender={bundle.sender_email}
      aria-current={focused ? "true" : undefined}
      aria-label={`${sender}: ${message.subject}`}
      onMouseDown={onFocus}
      className="border-b border-border px-5 pb-8 pt-6"
    >
      <div className="relative mx-auto max-w-[44rem]">
        {focused ? (
          <span
            aria-hidden
            className="absolute -left-4 top-0 h-16 w-[2px] rounded-full bg-primary"
          />
        ) : null}
        <header className="mb-4">
          <p className="flex flex-wrap items-baseline gap-x-2 text-[12.5px] text-muted-foreground">
            <span className="font-medium text-foreground/85">{sender}</span>
            <time
              dateTime={message.date}
              title={formatLongDate(message.date)}
              className="font-mono text-2xs tabular-nums"
            >
              {formatWhen(message.date)}
            </time>
            {message.pinned ? (
              <span className="inline-flex items-center gap-1 text-primary">
                <Pin aria-hidden className="size-3" /> Pinned
              </span>
            ) : null}
          </p>
          <h2 className="mt-1 text-balance text-[19px] font-semibold leading-snug tracking-tight text-foreground">
            {message.subject || "(no subject)"}
          </h2>
          <p className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground">
            <span data-testid="why-here">{whyHere(bundle.kind)}</span>
            <button
              type="button"
              onClick={() => openMoveSender(bundle)}
              className="inline-flex items-center gap-1 hover:text-foreground"
            >
              Move sender <KeyChip className="h-4 px-1">K</KeyChip>
            </button>
            <button
              type="button"
              onClick={() => void togglePin(message)}
              className="inline-flex items-center gap-1 hover:text-foreground"
            >
              {message.pinned ? "Unpin" : "Pin"} <KeyChip className="h-4 px-1">p</KeyChip>
            </button>
            <button
              type="button"
              onClick={() => openUnsubscribe(bundle, message)}
              className="inline-flex items-center gap-1 hover:text-foreground"
            >
              <MailX aria-hidden className="size-3" /> Unsubscribe
            </button>
            <button
              type="button"
              onClick={onOpen}
              className="inline-flex items-center gap-1 hover:text-foreground"
            >
              <ArrowUpRight aria-hidden className="size-3" /> Open conversation
            </button>
          </p>
        </header>
        <IssueBody issue={issue} />
      </div>
    </article>
  );
});

/**
 * The issue's body, fetched when it comes near the screen, through the same
 * thread read and sanitizer as the reader.
 */
function IssueBody({ issue }: { issue: ReadingIssue }) {
  const ref = useRef<HTMLDivElement>(null);
  const [near, setNear] = useState(false);
  const [remoteAllowed, setRemoteAllowed] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || near) return;
    if (typeof IntersectionObserver === "undefined") {
      setNear(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) setNear(true);
      },
      { rootMargin: "800px 0px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [near]);
  const thread = useQuery({
    queryKey: ["thread", issue.message.thread_id],
    queryFn: () => fetchThread(issue.message.thread_id),
    enabled: near,
    staleTime: 60_000,
  });
  const body = thread.data?.bodies.find((item) => item.message_id === issue.message.message_id);
  const row =
    thread.data?.messages.find((item) => item.id === issue.message.message_id) ??
    placeMessageRow(issue.bundle, issue.message);
  return (
    <div
      ref={ref}
      data-testid="issue-body"
      data-loaded={body ? "true" : undefined}
      className={cn("min-h-24", thread.isError && "text-muted-foreground")}
    >
      {thread.isError ? (
        <p className="text-[13px]">Couldn't load this issue: {thread.error.message}</p>
      ) : (
        <MessageContent
          message={row}
          body={body}
          view={body?.text_html ? "formatted" : "reader"}
          showQuotes={false}
          showSignature={false}
          remoteAllowed={remoteAllowed}
          onAllowRemote={() => setRemoteAllowed(true)}
          onShowFormatted={() => undefined}
          senderEmail={issue.bundle.sender_email}
        />
      )}
    </div>
  );
}
