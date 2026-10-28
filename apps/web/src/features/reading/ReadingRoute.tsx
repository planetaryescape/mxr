import { useQuery } from "@tanstack/react-query";
import { moveCommands, readingMoveSubject } from "@/features/arrivals/moveSubjects";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { READING_KEEP, READING_LATER, readingNotNow } from "@/features/mail-actions/notNow";
import { Link, Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { Bookmark } from "lucide-react";
import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import { FinishingMoment } from "@/features/low-tide/FinishingMoment";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { NextPlace } from "@/features/modes/NextPlace";
import { useModeDone } from "@/features/modes/modeDone";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { openMoveSenderFor } from "@/features/places/placeVerbs";
import { useRowSwipe, SwipeLayer, type SwipeLayerHandle } from "@/features/swipe/RowSwipe";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { pickSearch, useSearchParam } from "@/hooks/useSearchParam";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useLitReason } from "@/state/litReasonStore";
import { definedShortcut, stripKeys } from "@/lib/actions/definition";
import { usePageKeys } from "@/state/pageKeysStore";
import { useThreadModesMap } from "@/features/modes/membership";
import type { LampVia } from "@/components/Row";

import {
  fetchEdition,
  READING_KEY,
  useAccountScope,
  useMarginaliaStatus,
  type ReadingEdition,
  type ReadingItem,
  type ReadingSource,
} from "./api";
import { LetGoAllDialog, ReadingUnsubscribeDialog } from "./ReadingDialogs";
import { ReadingIssueRow, ReadingLinkRow, type RowHandlers } from "./ReadingRows";
import {
  editionEntries,
  letGoAllPlan,
  shownLinks,
  READING_LIST_SEARCH,
  visibleBands,
  type EditionEntry,
} from "./readingView";
import {
  answerLater,
  fetchLinked,
  letGo,
  openInMarginalia,
  putOnLater,
  rememberLayout,
} from "./readingVerbs";
import { ModeFrame } from "@/components/ModeFrame";
import { pageTabClass } from "@/components/Page";
import { PageFrame } from "@/components/PageFrame";
import { SectionHeading } from "@/components/SectionHeading";
import { listMotions } from "@/features/modes/listMotions";

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
    <PlaceLayout
      basePath="/reading"
      label="Reading"
      threadIds={threadIds}
      keepSearch={READING_LIST_SEARCH}
      wideReader
    >
      {params.itemKey ? <Outlet /> : <Edition status={edition} view={view} />}
    </PlaceLayout>
  );
}

/** The item an entry belongs to: itself, or a link's issue. */
const itemOf = (entry: EditionEntry | undefined) =>
  entry?.kind === "item" ? entry.item : entry?.parent;

type Dialog =
  | { kind: "unsubscribe"; source: ReadingSource }
  | { kind: "let-go-all"; threadIds: string[]; titles: string[] }
  | null;

function Edition({ status, view }: { status: ReturnType<typeof useEdition>; view?: "later" }) {
  const guide = useModeGuide("reading");
  const phase = useDelayedPending(status.isLoading);
  const data = status.data;
  const [dialog, setDialog] = useState<Dialog>(null);
  const hidden = useModeDone((s) => s.hidden.reading);
  // Reading is put to bed when its edition is let go (docs/blueprint/27). Read
  // items stay listed until they are let go or expire, so an emptied edition
  // is the finish.
  const finished = useLowTide(
    "reading",
    view === "later" || !data,
    data ? visibleBands(data, hidden).length > 0 : false,
    { daily: true },
  );
  return (
    <>
      <EditionHeader
        edition={data}
        guide={guide.data}
        view={view}
        onLetGoAll={() => data && openLetGoAll(data, setDialog)}
      />
      {phase !== "ready" ? (
        <PageSkeleton framed shown={phase === "skeleton"} />
      ) : status.isError ? (
        <PageError
          framed
          title="Couldn't load Reading"
          error={status.error}
          onRetry={() => void status.refetch()}
        />
      ) : data ? (
        <Bands
          edition={data}
          guide={guide.data}
          view={view}
          setDialog={setDialog}
          finished={finished}
        />
      ) : null}
      {dialog?.kind === "unsubscribe" ? (
        <ReadingUnsubscribeDialog source={dialog.source} onClose={() => setDialog(null)} />
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
    <PageFrame
      title={view === "later" ? "Later" : "Reading"}
      purpose={
        view === "later"
          ? "What you kept to read. It never fades."
          : (guide?.header ?? edition?.header ?? "")
      }
      tabs={
        <nav aria-label="Reading views" className="-mb-px flex h-8 gap-6">
          <Link
            to="/reading"
            title={`Edition (${definedShortcut("reading.edition")})`}
            aria-keyshortcuts={definedShortcut("reading.edition")}
            aria-current={view === "later" ? undefined : "page"}
            className={pageTabClass(view !== "later")}
          >
            Edition
          </Link>
          <Link
            to="/reading"
            search={{ view: "later" }}
            title={`Later (${definedShortcut("reading.later-shelf")})`}
            aria-keyshortcuts={definedShortcut("reading.later-shelf")}
            data-testid="later-count"
            aria-current={view === "later" ? "page" : undefined}
            className={pageTabClass(view === "later")}
          >
            <Bookmark aria-hidden className="mr-1 inline size-3.5" />
            Later <span className="type-meta ml-1 text-text-3">{later}</span>
          </Link>
        </nav>
      }
      primary={
        view !== "later" && anything ? (
          <Button
            size="sm"
            title={`Let go of all (${definedShortcut("reading.let-go-all")})`}
            aria-keyshortcuts={definedShortcut("reading.let-go-all")}
            onClick={onLetGoAll}
          >
            Let go of all
          </Button>
        ) : null
      }
    />
  );
}

function Bands({
  edition,
  guide,
  view,
  setDialog,
  finished,
}: {
  edition: ReadingEdition;
  guide?: ModeGuide;
  view?: "later";
  setDialog: (dialog: Dialog) => void;
  finished: boolean;
}) {
  const navigate = useNavigate();
  const marginalia = useMarginaliaStatus();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const hidden = useModeDone((s) => s.hidden.reading);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  // In the URL, so a copied link (and coming back from the reader) lands
  // on the same entry.
  const [cursorKey, setCursorKey] = useSearchParam("row");
  // Only a pointer fades the lamp (D101): a key must feel instant.
  const via = useRef<LampVia>("key");

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
  // A link to one of a digest's links unfolds that digest, once.
  const linkedEntry = useRef(cursorKey);
  useEffect(() => {
    const linked = linkedEntry.current;
    linkedEntry.current = null;
    if (!linked || entries.some((entry) => entry.key === linked)) return;
    const parent = bands
      .flatMap((band) => band.items)
      .find((item) => item.links?.some((link) => link.item_key === linked));
    if (parent) setExpanded((previous) => new Set(previous).add(parent.item_key));
  }, [bands, entries]);
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
    (key: string, at?: "highlights") => {
      setActivePane("reader");
      void navigate({
        to: "/reading/item/$itemKey",
        params: { itemKey: key },
        search: (prev) => ({ ...pickSearch(prev, READING_LIST_SEARCH), row: key, at }),
      });
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
      if (source) setDialog({ kind: "unsubscribe", source });
    },
    [edition, setDialog],
  );
  const handlers = useMemo<RowHandlers>(
    () => ({
      onPoint: (key) => {
        via.current = "pointer";
        setCursorKey(key);
      },
      onFocus: (key) => {
        via.current = "key";
        setCursorKey(key);
      },
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
    [later, letGoItem, read, setCursorKey, unsubscribe],
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
    if (!next) return;
    via.current = "key";
    setCursorKey(next.key);
  };
  // The lit entry's reason goes to the status bar; the row itself never grows a line.
  useLitReason(itemOf(current)?.why, null, itemOf(current)?.account_id);
  const litItem = itemOf(current);
  usePageKeys(
    // The email's reader owns the keys while it has focus, as it owns the scope.
    entries.length === 0 || (params.threadId && activePane === "reader")
      ? []
      : stripKeys(
          "reading",
          litItem?.unsubscribe_offer && view !== "later" ? [] : ["reading.unsubscribe"],
        ),
  );
  // The marks for "also in" modes, from the daemon's membership.
  const memberThreads = useMemo(
    () =>
      [
        ...new Set(
          entries.flatMap((entry) => (entry.kind === "item" ? [entry.item.thread_id] : [])),
        ),
      ].slice(0, 100),
    [entries],
  );
  const memberships = useThreadModesMap(memberThreads).data;
  useShortcutScope("reading", !params.threadId || activePane !== "reader");
  useScopeController("reading", {
    down: () => move(1),
    up: () => move(-1),
    ...listMotions(move),
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
    // Not now (`b`): Later first, then a time that snoozes the issue's email.
    later: () => {
      if (!current) return;
      const subject = readingMoveSubject(current);
      if (!subject) return;
      const key = current.key;
      openMailDialog({
        kind: "not-now",
        picker: readingNotNow({
          messageId: subject.messageId,
          title: subject.label,
          later: {
            ...(view === "later" ? READING_KEEP : READING_LATER),
            run: () => {
              fadingHint.dismiss();
              later(key);
            },
          },
        }),
      });
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
    // The tabs: `1` the edition, `2` Later, `3` the lit item's highlights
    // (there is no page of every highlight yet).
    edition: () => void navigate({ to: "/reading", search: {} }),
    laterShelf: () => void navigate({ to: "/reading", search: { view: "later" } }),
    highlights: () => {
      if (current) read(current.key, "highlights");
    },
    // The daemon knows whether it was sent before; then it only opens.
    marginalia: () => {
      if (!current) return;
      void openInMarginalia(current.key, null, {
        connected: marginalia.data?.connected ?? false,
        onConnect: () =>
          void navigate({ to: "/settings/$section", params: { section: "marginalia" } }),
      });
    },
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
        <PageEmpty
          framed
          glyph="reading"
          title="Nothing on Later"
          body="Press b on an item or a link to keep it here. Later never fades, and a link's article is saved so it reads offline."
        />
      );
    }
    return (
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto">
        <PageEmpty
          framed
          glyph={finished ? undefined : "reading"}
          moment={finished ? <FinishingMoment /> : undefined}
          title={empty?.never_had_any ? "Nothing here yet" : "You're current"}
          body={
            <>
              {empty?.line ?? guide?.clear_for_now ?? ""}
              <NextPlace current="reading" />
            </>
          }
          action={
            edition.later_count > 0 ? (
              <Link
                to="/reading"
                search={{ view: "later" }}
                className="text-sm underline decoration-border-strong underline-offset-4 hover:decoration-primary"
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
      <ModeFrame>
        {edition.empty && view !== "later" ? (
          <p data-testid="reading-clear" className="mx-5 mt-3 text-sm text-foreground/90">
            {edition.empty.line}
          </p>
        ) : null}
        {bands.map((band, bandIndex) => (
          <section key={band.band} aria-label={band.label} data-testid={`band-${band.band}`}>
            {bandIndex === 1 && sinceFirst && edition.left_off_here && view !== "later" ? (
              <p
                data-testid="left-off-here"
                className="mx-5 mt-5 flex items-center gap-3 text-2xs text-muted-foreground"
              >
                <span aria-hidden className="h-px flex-1 bg-border" />
                You left off here
                <span aria-hidden className="h-px flex-1 bg-border" />
              </p>
            ) : null}
            {view !== "later" ? (
              <SectionHeading className="mx-5 mb-1 mt-5">
                {band.label}
                {band.note ? (
                  <span className="ml-2 normal-case tracking-normal">{band.note}</span>
                ) : null}
              </SectionHeading>
            ) : null}
            {band.band === "fading" && fadingHint.hint ? (
              <AnchoredHint
                hint={fadingHint.hint}
                onDismiss={fadingHint.dismiss}
                className="mx-5 mb-2"
              />
            ) : null}
            <ul className="grid grid-cols-[minmax(0,1fr)]">
              {band.items.map((item) => {
                const { shown, more } = shownLinks(item.links ?? [], expanded.has(item.item_key));
                return (
                  <Fragment key={item.item_key}>
                    <ReadingIssueRow
                      item={item}
                      index={position.get(item.item_key) ?? 0}
                      lit={current?.key === item.item_key}
                      via={via.current}
                      fading={band.band === "fading"}
                      shelf={view === "later"}
                      modes={memberships?.get(item.thread_id)}
                      handlers={handlers}
                    />
                    {shown.map((link) => (
                      <ReadingLinkRow
                        key={link.item_key}
                        link={link}
                        index={position.get(link.item_key) ?? -1}
                        lit={current?.key === link.item_key}
                        via={via.current}
                        handlers={handlers}
                        hint={
                          linkHint.hint &&
                          current?.kind === "link" &&
                          current.key === link.item_key ? (
                            <AnchoredHint hint={linkHint.hint} onDismiss={linkHint.dismiss} />
                          ) : undefined
                        }
                      />
                    ))}
                    {more > 0 ? (
                      <li className="ml-6 mx-2">
                        <button
                          type="button"
                          onClick={() => handlers.onToggleLinks(item.item_key)}
                          className="type-meta h-8 px-3 text-text-3 hover:text-text-1"
                        >
                          + {more} more
                        </button>
                      </li>
                    ) : null}
                    {view === "later" && item.still_want_it ? (
                      <li
                        data-testid="still-want-it"
                        className="type-body mx-2 flex items-center gap-3 px-3 pb-2 text-text-2"
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
                      </li>
                    ) : null}
                  </Fragment>
                );
              })}
            </ul>
          </section>
        ))}
      </ModeFrame>
    </div>
  );
}
