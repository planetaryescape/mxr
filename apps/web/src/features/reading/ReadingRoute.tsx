import { useQuery } from "@tanstack/react-query";
import { moveCommands, readingMoveSubject } from "@/features/arrivals/moveSubjects";
import { Link, Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { Bookmark, Newspaper, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { useModeDone } from "@/features/modes/modeDone";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { openMoveSenderFor } from "@/features/places/placeVerbs";
import { useRowSwipe, SwipeLayer, type SwipeLayerHandle } from "@/features/swipe/RowSwipe";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";

import {
  fetchEdition,
  READING_KEY,
  useAccountScope,
  type ReadingEdition,
  type ReadingItem,
  type ReadingSource,
} from "./api";
import { LetGoAllDialog, ReadingUnsubscribeDialog } from "./ReadingDialogs";
import { ReadingItemCard, type CardHandlers } from "./ReadingItemCard";
import { editionEntries, letGoAllPlan, visibleBands, type EditionEntry } from "./readingView";
import { answerLater, fetchLinked, letGo, putOnLater, rememberLayout } from "./readingVerbs";

export const EDITION_KEY = (scope: string) => [...READING_KEY, "edition", scope] as const;

/**
 * The edition for the account scope. The first fetch on each visit
 * records that Reading was opened, so the next visit's "Since you were
 * last here" starts from this one.
 */
export function useEdition() {
  const account = useAccountScope();
  const scope = account ?? "all";
  const visited = useRef(new Set<string>());
  return useQuery({
    queryKey: EDITION_KEY(scope),
    queryFn: () => {
      const markVisit = !visited.current.has(scope);
      visited.current.add(scope);
      return fetchEdition(account, markVisit);
    },
    staleTime: 15_000,
    refetchInterval: 120_000,
  });
}

/** The source behind an item, from the edition's sources. */
export function sourceOf(edition: ReadingEdition | undefined, item: ReadingItem) {
  return edition?.sources.find(
    (source) => source.account_id === item.account_id && source.sender_email === item.sender_email,
  );
}

/**
 * Reading: newsletters you chose, as an edition you visit. Three bands by
 * time, ranked inside by how much you read each source; Later is the only
 * count. An item opens in the reader (`/reading/item/<key>`), the email as
 * sent at `/reading/<thread>`.
 */
export function ReadingRoute({ view }: { view?: "later" }) {
  const params = useParams({ strict: false }) as { itemKey?: string; threadId?: string };
  const edition = useEdition();
  const threadIds = useCallback(
    () => edition.data?.bands.flatMap((band) => band.items.map((item) => item.thread_id)) ?? [],
    [edition.data],
  );
  return (
    <PlaceLayout basePath="/reading" label="Reading" threadIds={threadIds} wideReader>
      {params.itemKey ? <Outlet /> : <Edition status={edition} view={view} />}
    </PlaceLayout>
  );
}

/** The item an entry belongs to: itself, or a link's issue. */
const itemOf = (entry: EditionEntry | undefined) =>
  entry?.kind === "item" ? entry.item : entry?.parent;

type Dialog =
  | { kind: "unsubscribe"; source: ReadingSource; messageId: string }
  | { kind: "let-go-all"; threadIds: string[]; titles: string[] }
  | null;

function Edition({ status, view }: { status: ReturnType<typeof useEdition>; view?: "later" }) {
  const guide = useModeGuide("reading");
  const phase = useDelayedPending(status.isLoading);
  const data = status.data;
  const [dialog, setDialog] = useState<Dialog>(null);
  return (
    <>
      <EditionHeader
        edition={data}
        guide={guide.data}
        view={view}
        onLetGoAll={() => data && openLetGoAll(data, setDialog)}
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
      ) : data ? (
        <Bands edition={data} guide={guide.data} view={view} setDialog={setDialog} />
      ) : null}
      {dialog?.kind === "unsubscribe" ? (
        <ReadingUnsubscribeDialog
          source={dialog.source}
          messageId={dialog.messageId}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === "let-go-all" ? (
        <LetGoAllDialog
          threadIds={dialog.threadIds}
          titles={dialog.titles}
          onClose={() => setDialog(null)}
        />
      ) : null}
    </>
  );
}

function openLetGoAll(edition: ReadingEdition, setDialog: (dialog: Dialog) => void) {
  const hidden = useModeDone.getState().hidden.reading;
  const plan = letGoAllPlan(visibleBands(edition, hidden));
  if (plan.threadIds.length > 0) setDialog({ kind: "let-go-all", ...plan });
}

function EditionHeader({
  edition,
  guide,
  view,
  onLetGoAll,
}: {
  edition?: ReadingEdition;
  guide?: ModeGuide;
  view?: "later";
  onLetGoAll: () => void;
}) {
  const later = edition?.later_count ?? 0;
  const anything = (edition?.bands.length ?? 0) > 0;
  return (
    <header className="flex shrink-0 flex-wrap items-baseline gap-x-3 gap-y-1 border-b border-border px-5 pb-3 pt-4">
      <h1 className="text-[17px] font-semibold tracking-tight text-foreground">
        {view === "later" ? "Later" : "Reading"}
      </h1>
      <p data-testid="mode-header" className="min-w-0 text-[12.5px] text-muted-foreground">
        {view === "later"
          ? "What you kept to read. It never fades."
          : (guide?.header ?? edition?.header ?? "")}
      </p>
      <div className="ml-auto flex items-center gap-3">
        {view === "later" ? (
          <Link to="/reading" className="text-[12.5px] text-muted-foreground hover:text-foreground">
            Back to the edition
          </Link>
        ) : (
          <Link
            to="/reading"
            search={{ view: "later" }}
            data-testid="later-count"
            className="inline-flex items-center gap-1.5 text-[12.5px] text-foreground/90 hover:text-foreground"
          >
            <Bookmark aria-hidden className="size-3.5" /> Later{" "}
            <span className="tabular-nums">{later}</span>
          </Link>
        )}
        {view !== "later" && anything ? (
          <Button variant="outline" size="xs" onClick={onLetGoAll}>
            Let go of all <KeyChip className="ml-1 h-4 px-1">A</KeyChip>
          </Button>
        ) : null}
      </div>
    </header>
  );
}

function Bands({
  edition,
  guide,
  view,
  setDialog,
}: {
  edition: ReadingEdition;
  guide?: ModeGuide;
  view?: "later";
  setDialog: (dialog: Dialog) => void;
}) {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const hidden = useModeDone((s) => s.hidden.reading);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [cursorKey, setCursorKey] = useState<string | null>(null);

  const bands = useMemo(
    () =>
      view === "later"
        ? edition.later.length > 0
          ? [{ band: "earlier" as const, label: "Later", items: edition.later }]
          : []
        : visibleBands(edition, hidden),
    [edition, hidden, view],
  );
  const entries = useMemo(() => editionEntries(bands, expanded), [bands, expanded]);
  const index = Math.max(
    0,
    entries.findIndex((entry) => entry.key === cursorKey),
  );
  const current: EditionEntry | undefined = entries[index];
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  // Each hint shows when the cursor first reaches its element: the Fading
  // band, and a link under a digest. Acting there dismisses it.
  const onEdition = view !== "later";
  const fadingHint = useHint("reading", "reading.fading", {
    ready: onEdition && current?.band === "fading",
  });
  const linkHint = useHint("reading", "reading.link", {
    ready: onEdition && current?.kind === "link",
  });
  const closeHint = useActiveHintDismiss();

  const read = useCallback(
    (key: string) => {
      setActivePane("reader");
      void navigate({ to: "/reading/item/$itemKey", params: { itemKey: key } });
    },
    [navigate, setActivePane],
  );
  const later = useCallback(
    (key: string) => {
      const entry = entries.find((candidate) => candidate.key === key);
      if (view === "later") {
        void answerLater(key, true);
        return;
      }
      if (entry?.kind === "link") {
        void putOnLater(key, {
          domain: entry.link.domain,
          tracked: entry.link.tracked,
          cached: entry.link.article_cached,
        });
      } else if (entry?.kind === "item") {
        const item = entry.item;
        void putOnLater(
          key,
          item.url
            ? { domain: item.domain, tracked: item.tracked, cached: item.article_cached }
            : undefined,
        );
      }
    },
    [entries, view],
  );
  const letGoItem = useCallback(
    (item: ReadingItem) => {
      if (view === "later") void answerLater(item.item_key, false);
      else void letGo([item.thread_id]);
    },
    [view],
  );
  const unsubscribe = useCallback(
    (item: ReadingItem) => {
      const source = sourceOf(edition, item);
      if (source) setDialog({ kind: "unsubscribe", source, messageId: item.message_id });
    },
    [edition, setDialog],
  );
  const handlers = useMemo<CardHandlers>(
    () => ({
      onFocus: setCursorKey,
      onRead: read,
      onLater: later,
      onLetGo: letGoItem,
      onUnsubscribe: unsubscribe,
      onToggleLinks: (key) =>
        setExpanded((previous) => {
          const next = new Set(previous);
          if (next.has(key)) next.delete(key);
          else next.add(key);
          return next;
        }),
    }),
    [later, letGoItem, read, unsubscribe],
  );

  // Touch: swipe right keeps it for later, left lets it go.
  const entriesRef = useRef(entries);
  entriesRef.current = entries;
  const swipeLayer = useRef<SwipeLayerHandle>(null);
  useRowSwipe(
    listRef,
    {
      rowSelector: '[data-testid="reading-item"]',
      layer: swipeLayer,
      resolve: (element) => {
        const entry = entriesRef.current[Number(element.dataset.index)];
        if (entry?.kind !== "item") return null;
        return {
          actions: { right: "later", left: "letgo" },
          commit: (action) => (action === "later" ? later(entry.key) : letGoItem(entry.item)),
        };
      },
    },
    entries.length > 0,
  );

  const move = (delta: number) => {
    const next = entries[Math.min(entries.length - 1, Math.max(0, index + delta))];
    if (next) setCursorKey(next.key);
  };
  useShortcutScope("reading", !params.threadId || activePane !== "reader");
  useScopeController("reading", {
    down: () => move(1),
    up: () => move(-1),
    read: () => {
      if (!current) return;
      linkHint.dismiss();
      read(current.key);
    },
    article: () => {
      if (!current) return;
      linkHint.dismiss();
      const target = current.kind === "link" ? current.link : current.item;
      if (!target.url) {
        read(current.key);
        return;
      }
      void fetchLinked(current.key, target.domain, target.tracked).then((fetched) => {
        if (fetched && !fetched.error) read(current.key);
      });
    },
    later: () => {
      if (!current) return;
      fadingHint.dismiss();
      later(current.key);
    },
    letGo: () => {
      fadingHint.dismiss();
      const item = itemOf(current);
      if (item) letGoItem(item);
    },
    unsubscribe: () => {
      const item = itemOf(current);
      if (item) unsubscribe(item);
    },
    original: () => {
      const item = itemOf(current);
      if (!item) return;
      void rememberLayout(item.account_id, item.sender_email, true).then(() => read(item.item_key));
    },
    letGoAll: () => view !== "later" && openLetGoAll(edition, setDialog),
    // `K` stays "move sender to a kind"; only the email moves from here.
    moveToMode: moveCommands(() => (current ? readingMoveSubject(current) : null)).moveToMode,
    openEmail: () => {
      const item = itemOf(current);
      if (!item) return;
      setActivePane("reader");
      void navigate({ to: "/reading/$threadId", params: { threadId: item.thread_id } });
    },
    moveSender: () => {
      const item = itemOf(current);
      if (item) void openMoveSenderFor(item.message_id, item.source);
    },
    laterShelf: () =>
      void navigate({ to: "/reading", search: view === "later" ? {} : { view: "later" } }),
    closeHint,
  });

  const position = useMemo(() => {
    const map = new Map<string, number>();
    entries.forEach((entry, at) => map.set(entry.key, at));
    return map;
  }, [entries]);

  if (bands.length === 0) {
    const empty = edition.empty;
    if (view === "later") {
      return (
        <Centered
          icon={<Bookmark className="size-6" />}
          title="Nothing on Later"
          body="Press b on an item or a link to keep it here. Later never fades, and a link's article is saved so it reads offline."
        />
      );
    }
    return (
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto">
        <Centered
          icon={<Newspaper className="size-6" />}
          title={empty?.never_had_any ? "Nothing here yet" : "You're current"}
          body={empty?.line ?? guide?.clear_for_now ?? ""}
          action={
            edition.later_count > 0 ? (
              <Link
                to="/reading"
                search={{ view: "later" }}
                className="text-[13px] underline decoration-border-strong underline-offset-4 hover:decoration-primary"
              >
                Open Later
              </Link>
            ) : undefined
          }
        />
      </div>
    );
  }

  const sinceFirst = bands[0]?.band === "since_last_visit";
  return (
    <div
      ref={listRef}
      className="relative min-h-0 flex-1 overflow-y-auto pb-10"
      data-testid="reading-edition"
    >
      <SwipeLayer ref={swipeLayer} />
      {edition.empty && view !== "later" ? (
        <p data-testid="reading-clear" className="mx-5 mt-3 text-[13px] text-foreground/90">
          {edition.empty.line}
        </p>
      ) : null}
      <div className="max-w-[60rem]">
        {bands.map((band, bandIndex) => (
          <section key={band.band} aria-label={band.label} data-testid={`band-${band.band}`}>
            {bandIndex === 1 && sinceFirst && edition.left_off_here && view !== "later" ? (
              <p
                data-testid="left-off-here"
                className="mx-5 mt-5 flex items-center gap-3 text-[11.5px] text-muted-foreground"
              >
                <span aria-hidden className="h-px flex-1 bg-border" />
                You left off here
                <span aria-hidden className="h-px flex-1 bg-border" />
              </p>
            ) : null}
            {view !== "later" ? (
              <h2 className="mx-5 mb-1 mt-5 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
                {band.label}
                {band.note ? (
                  <span className="ml-2 normal-case tracking-normal">{band.note}</span>
                ) : null}
              </h2>
            ) : null}
            {band.band === "fading" && fadingHint.hint ? (
              <AnchoredHint
                hint={fadingHint.hint}
                onDismiss={fadingHint.dismiss}
                className="mx-5 mb-2"
              />
            ) : null}
            {band.items.map((item) => (
              <div key={item.item_key}>
                <ReadingItemCard
                  item={item}
                  index={position.get(item.item_key) ?? 0}
                  linkIndex={position}
                  focusedKey={current?.key ?? null}
                  expanded={expanded.has(item.item_key)}
                  fading={band.band === "fading"}
                  shelf={view === "later"}
                  handlers={handlers}
                  linkHint={
                    linkHint.hint && current?.kind === "link" && current.parent === item
                      ? {
                          key: current.key,
                          node: <AnchoredHint hint={linkHint.hint} onDismiss={linkHint.dismiss} />,
                        }
                      : undefined
                  }
                />
                {view === "later" && item.still_want_it ? (
                  <p
                    data-testid="still-want-it"
                    className="mx-9 -mt-1 mb-2 flex items-center gap-3 text-[12.5px] text-muted-foreground"
                  >
                    Saved over a month ago. Still want it?
                    <Button
                      size="xs"
                      variant="outline"
                      onClick={() => void answerLater(item.item_key, true)}
                    >
                      Keep
                    </Button>
                    <Button
                      size="xs"
                      variant="outline"
                      onClick={() => void answerLater(item.item_key, false)}
                    >
                      Let go
                    </Button>
                  </p>
                ) : null}
              </div>
            ))}
          </section>
        ))}
        {guide && view !== "later" ? <KeyLine guide={guide} /> : null}
      </div>
    </div>
  );
}

/** Keys with their verbs at the point of use, from the mode's own table. */
function KeyLine({ guide }: { guide: ModeGuide }) {
  const keys = guide.keys.filter((key) => !["?", "u", "h", "o"].includes(key.key));
  return (
    <p className="mx-5 mt-8 hidden flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground md:flex">
      {keys.map((key) => (
        <span key={key.key} className="inline-flex items-center gap-1">
          <KeyChip>{key.key === "Enter" ? "↵" : key.key}</KeyChip> {key.verb}
        </span>
      ))}
      <span className="inline-flex items-center gap-1">
        <KeyChip>B</KeyChip> the Later shelf
      </span>
    </p>
  );
}
