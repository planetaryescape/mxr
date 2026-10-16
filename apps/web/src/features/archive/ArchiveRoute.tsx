import { useParams, useSearch } from "@tanstack/react-router";
import { ArrowLeft, SlidersHorizontal, X } from "lucide-react";
import { toast } from "sonner";
import { Fragment, useCallback, useEffect, useId, useMemo, useRef, useState } from "react";

import { Segmented } from "@/components/Segmented";
import type { LampVia } from "@/components/Row";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { ModeFrame } from "@/components/ModeFrame";
import { PageFrame } from "@/components/PageFrame";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useReplaceSearch, useSearchParam } from "@/hooks/useSearchParam";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane } from "@/hooks/useSplitPane";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import {
  useAnswer,
  useLedger,
  useRecord,
  useSubscriptions,
  type RecordData,
  type RecordAnswer,
  type RecordFilter,
  type RecordLedger,
  type RecordSubscription,
} from "./api";
import {
  copyAmount,
  copyReference,
  copyText,
  markChecked,
  markNotRecord,
  openDocument,
} from "./archiveVerbs";
import { filterFromSearch, searchFromFilter, type ArchiveSearch } from "./archiveSearch";
import { FacetsPanel } from "./FacetsPanel";
import { activeChip, answerList, groupByMonth, KIND_CHIPS, stepYear } from "./ledger";
import { AnswerCard, LedgerRow, MatchesHeader, RecordCard } from "./RecordParts";
import { SubscriptionCard, SubscriptionsList } from "./Subscriptions";
import { subscriptionAmountCopy } from "./subscriptionRows";
import { useLitReason } from "@/state/litReasonStore";
import { usePageKeys } from "@/state/pageKeysStore";
import { SectionHeading } from "@/components/SectionHeading";

const PAGE = 200;
const SUBSCRIPTIONS_SEGMENT = "subscriptions";
/** The kind filter and the subscriptions view are one either-or choice. */
const KIND_SEGMENTS: [string, string][] = [
  ...KIND_CHIPS.map((chip): [string, string] => [chip.id, chip.label]),
  [SUBSCRIPTIONS_SEGMENT, "Subscriptions"],
];
const openDocumentFor = (record: RecordData) => void openDocument(record);
/** The ledger of records, or the subscriptions found in them. */
type ArchiveSection = "ledger" | "subscriptions";

/** Coming-up kinds that are about a subscription, not a dated record. */
const SUBSCRIPTION_SIGNALS = new Set(["price_change", "missed_charge", "renewal_approaching"]);
/** The record card beside the ledger. */
const CARD_PANE_SIZE = { defaultSize: "38%", minSize: "20rem", maxSize: "36rem" };

/** Every ledger param stays while a record's email opens beside it. */
const ARCHIVE_LIST_SEARCH: readonly (keyof ArchiveSearch)[] = [
  "record",
  "section",
  "sub",
  "q",
  "year",
  "issuer",
  "kinds",
  "checked",
  "pdf",
  "group",
  "min",
  "max",
];

/**
 * Archive: a filing cabinet you ask questions of. The answer box is the
 * default focus; below it, a ledger of records by month with counts and
 * totals, newest first, and the selected record's card beside it. A row
 * is a record (an order's three emails are one row), never an email.
 */
export function ArchiveRoute() {
  const ids = useRef<string[]>([]);
  const threadIds = useCallback(() => ids.current, []);
  return (
    <PlaceLayout
      basePath="/archive"
      label="Archive"
      threadIds={threadIds}
      keepSearch={ARCHIVE_LIST_SEARCH}
    >
      <ArchiveView onThreads={(next) => (ids.current = next)} />
    </PlaceLayout>
  );
}

function useDebounced(value: string, ms: number): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), ms);
    return () => window.clearTimeout(timer);
  }, [value, ms]);
  return debounced;
}

function ArchiveView({ onThreads }: { onThreads: (ids: string[]) => void }) {
  const account = useUiPrefs((s) => s.accountScope);
  // The filter, section and question live in the URL, so a copied link
  // reopens the same view.
  const search = useSearch({ from: "/archive" });
  const replaceSearch = useReplaceSearch();
  // Keyed on the filter's value: a new search object with the same filter
  // keeps the same ledger query.
  const filterKey = JSON.stringify(filterFromSearch(search));
  const filter = useMemo(() => JSON.parse(filterKey) as RecordFilter, [filterKey]);
  const section: ArchiveSection = search.section ?? "ledger";
  const setSection = useCallback(
    (next: ArchiveSection) =>
      replaceSearch({ section: next === "subscriptions" ? next : undefined }),
    [replaceSearch],
  );
  const [limit, setLimit] = useState(PAGE);
  const ledger = useLedger(filter, limit);
  const guide = useModeGuide("archive");
  const phase = useDelayedPending(ledger.isLoading);
  const [query, setQuery] = useState(search.q ?? "");
  const asked = useDebounced(query, 250);
  const askedText = asked.trim();
  // The URL takes the question once typing settles, not every keystroke.
  useEffect(() => replaceSearch({ q: askedText || undefined }), [askedText, replaceSearch]);
  // "Show all" and "Show more" belong to the query they were pressed on: a
  // new query decides for itself between an answer and a list.
  const [showAllFor, setShowAllFor] = useState<string | null>(null);
  const [listPage, setListPage] = useState({ query: "", limit: PAGE });
  const typed = useAnswer(asked, false, {
    list: showAllFor === askedText,
    limit: listPage.query === askedText ? listPage.limit : PAGE,
  });
  // Enter on a query no record matches searches all mail, once.
  const [searched, setSearched] = useState("");
  const searchingAll = searched !== "" && searched === askedText && !typed.data?.answer;
  const all = useAnswer(searchingAll ? searched : "", true);
  const answer = searchingAll ? all : typed;
  // Cleared in the box: the answer goes at once, not after the debounce.
  const shownAnswer = query.trim() && askedText ? (answer.data ?? typed.data) : undefined;
  const list = answerList(shownAnswer);
  const records = useMemo(() => list?.records ?? ledger.data?.records ?? [], [list, ledger.data]);
  useEffect(() => {
    onThreads(records.map((record) => record.thread_id).filter((id): id is string => Boolean(id)));
  }, [onThreads, records]);

  return (
    <>
      <ArchiveHeader ledger={ledger.data} guide={guide.data} />
      {phase !== "ready" ? (
        <PageSkeleton framed shown={phase === "skeleton"} />
      ) : ledger.isError ? (
        <PageError
          framed
          title="Couldn't load Archive"
          error={ledger.error}
          onRetry={() => void ledger.refetch()}
        />
      ) : ledger.data ? (
        <Ledger
          ledger={ledger.data}
          guide={guide.data}
          account={account}
          filter={filter}
          setFilter={(next) => {
            replaceSearch({ ...searchFromFilter(next), section: undefined });
            setLimit(PAGE);
          }}
          section={section}
          setSection={setSection}
          query={query}
          setQuery={(next) => {
            setQuery(next);
            // Leaving the query behind drops its "Show all".
            if (next.trim() !== showAllFor) setShowAllFor(null);
          }}
          answer={shownAnswer}
          answerPending={answer.isFetching && Boolean(askedText)}
          onSearchAll={() => setSearched(query.trim())}
          onShowAll={() => setShowAllFor(askedText)}
          onMore={() => setLimit((current) => current + PAGE)}
          onMoreMatches={() =>
            setListPage((current) => ({
              query: askedText,
              limit: (current.query === askedText ? current.limit : PAGE) + PAGE,
            }))
          }
        />
      ) : null}
    </>
  );
}

/** The mode's name, its job in one line, and how many records it holds. */
function ArchiveHeader({ ledger, guide }: { ledger?: RecordLedger; guide?: ModeGuide }) {
  return (
    <>
      <PageFrame
        width="wide"
        title="Archive"
        purpose={
          guide?.header ??
          ledger?.header ??
          "Receipts, orders, bookings and documents. Ask for what you need."
        }
        freshness={
          ledger && ledger.total > 0 ? (
            <span data-testid="records-total">{plural(ledger.total, "record")}</span>
          ) : null
        }
      />
      {ledger?.first_run ? (
        <ModeFrame width="wide" className="px-5 pt-2">
          <p data-testid="records-first-run" className="type-meta text-text-3">
            {ledger.first_run.line}
          </p>
        </ModeFrame>
      ) : null}
    </>
  );
}

interface LedgerProps {
  ledger: RecordLedger;
  guide?: ModeGuide;
  account: string | null;
  filter: RecordFilter;
  setFilter: (filter: RecordFilter) => void;
  section: ArchiveSection;
  setSection: (section: ArchiveSection) => void;
  query: string;
  setQuery: (query: string) => void;
  answer?: RecordAnswer;
  answerPending: boolean;
  /** Enter on a query no record matches: search all mail. */
  onSearchAll: () => void;
  /** List every match of the query on screen. */
  onShowAll: () => void;
  onMore: () => void;
  onMoreMatches: () => void;
}

function Ledger({
  ledger,
  guide,
  account,
  filter,
  setFilter,
  section,
  setSection,
  query,
  setQuery,
  answer,
  answerPending,
  onSearchAll,
  onShowAll,
  onMore,
  onMoreMatches,
}: LedgerProps) {
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const singlePane = useMediaQuery(SINGLE_PANE_QUERY);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputId = useId();
  // A listed query's matches stand in for the ledger's rows.
  const list = answerList(answer);
  const records = list?.records ?? ledger.records;
  const months = list?.months ?? ledger.months;
  const [cursorId, setCursorId] = useSearchParam("record");
  // Only a pointer fades the lamp (D101): a key must feel instant.
  const via = useRef<LampVia>("key");
  // While an answer is on screen and the cursor hasn't moved, y, Enter and
  // o act on the answer's record.
  const [onAnswer, setOnAnswer] = useState(false);
  const [cardOpen, setCardOpen] = useState(false);
  const [facetsOpen, setFacetsOpen] = useState(false);
  const index = Math.max(
    0,
    records.findIndex((record) => record.id === cursorId),
  );
  const rowRecord: RecordData | undefined = records[index];
  const answerRecord = list ? undefined : answer?.answer?.record;
  const current = onAnswer && answerRecord ? answerRecord : rowRecord;
  const showingSubscriptions = section === "subscriptions";
  const full = useRecord(
    showingSubscriptions || (singlePane && !cardOpen) ? null : (current?.id ?? null),
  );
  const cardRecord = full.data && full.data.id === current?.id ? full.data : current;
  const subscriptions = useSubscriptions(showingSubscriptions);
  // The daemon sends live ones before ended ones, the order they are drawn.
  const subscriptionRows = useMemo(
    () => subscriptions.data?.subscriptions ?? [],
    [subscriptions.data],
  );
  const [subscriptionId, setSubscriptionId] = useSearchParam("sub");
  const subscriptionIndex = Math.max(
    0,
    subscriptionRows.findIndex((row) => row.id === subscriptionId),
  );
  const subscription: RecordSubscription | undefined = subscriptionRows[subscriptionIndex];
  const showSideCard = !singlePane && !params.threadId && cardRecord;
  const showSideSubscription =
    !singlePane && !params.threadId && showingSubscriptions && Boolean(subscription);
  const pane = useSplitPane("archive-card", CARD_PANE_SIZE, {
    active: Boolean(showSideCard) || showSideSubscription,
  });

  // A card takes the keys; a list puts the cursor on its best match. Only a
  // new list moves the cursor: a refetch or "Show more" leaves it.
  const bestId = list?.top_record_id ?? null;
  const listQuery = list ? answer?.query : undefined;
  // A link's own record wins over the best match of the link's own question.
  const linkedRecord = useRef({ id: cursorId, query: query.trim() });
  const listed = useRef(records);
  listed.current = records;
  useEffect(() => {
    if (!bestId) return;
    const linked = linkedRecord.current;
    if (
      linked.id &&
      linked.query === listQuery &&
      listed.current.some((record) => record.id === linked.id)
    )
      return;
    linkedRecord.current = { id: null, query: "" };
    setCursorId(bestId);
    setOnAnswer(false);
  }, [bestId, listQuery, setCursorId]);
  useEffect(() => {
    if (!bestId && answer?.answer) setOnAnswer(true);
  }, [answer, bestId]);
  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${showingSubscriptions ? subscriptionIndex : index}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [index, subscriptionIndex, showingSubscriptions]);

  // One hint under the first record row, one under the first answer.
  const answerHint = useHint("archive", "archive.answer", { ready: Boolean(answer?.answer) });
  const recordHint = useHint("archive", "archive.record", {
    ready: records.length > 0 && !answer?.answer,
    alone: records.length === 1,
  });
  const { dismiss: dismissRecordHint } = recordHint;
  const closeHint = useActiveHintDismiss();

  const select = useCallback(
    (record: RecordData) => {
      setCursorId(record.id);
      setOnAnswer(false);
    },
    [setCursorId],
  );
  // A click on a row: the lamp fades to it. Keys go through `select` and stay instant.
  const selectByPointer = useCallback(
    (record: RecordData) => {
      via.current = "pointer";
      select(record);
    },
    [select],
  );
  const openCard = useCallback(
    (record: RecordData) => {
      selectByPointer(record);
      setCardOpen(true);
    },
    [selectByPointer],
  );
  const openEmail = useCallback(
    (record?: RecordData) => {
      dismissRecordHint();
      if (record?.thread_id) nav?.open(record.thread_id);
    },
    [dismissRecordHint, nav],
  );
  const selectSubscription = useCallback(
    (row: RecordSubscription) => {
      setSubscriptionId(row.id);
      if (singlePane) setCardOpen(true);
    },
    [setSubscriptionId, singlePane],
  );
  const openSubscriptionEmail = useCallback(
    (row?: RecordSubscription) => {
      if (row?.thread_id) nav?.open(row.thread_id);
    },
    [nav],
  );
  const subscriptionIssuer = (row?: RecordSubscription) => {
    if (row) setFilter({ issuer: row.issuer });
  };
  const copySubscriptionAmount = (row?: RecordSubscription) => {
    const text = row ? subscriptionAmountCopy(row) : null;
    if (text) void copyText(text, "amount");
  };
  const move = (delta: number) => {
    if (showingSubscriptions) {
      const next =
        subscriptionRows[
          Math.min(subscriptionRows.length - 1, Math.max(0, subscriptionIndex + delta))
        ];
      if (next) setSubscriptionId(next.id);
      return;
    }
    const base = onAnswer ? -1 + (delta > 0 ? 1 : 0) : index + delta;
    const next = records[Math.min(records.length - 1, Math.max(0, base))];
    if (!next) return;
    via.current = "key";
    select(next);
  };
  const years = ledger.facets.years.map((year) => Number(year.value)).filter(Number.isFinite);
  const stepTo = (delta: -1 | 1) =>
    setFilter({ ...filter, year: stepYear(years, filter.year, delta) });
  const clearSearch = () => {
    setQuery("");
    inputRef.current?.focus();
  };
  const issuerPage = (record?: RecordData) => {
    if (record?.issuer) setFilter({ issuer: record.issuer });
  };
  const listIssuerPage = (issuer: string) => {
    setQuery("");
    setFilter({ issuer });
  };
  const copyRef = () => {
    answerHint.dismiss();
    if (onAnswer && answer?.answer) void copyText(answer.answer.copy, "answer");
    else if (current) void copyReference(current);
  };
  const recordOnlyAction = () => {
    if (!showingSubscriptions) return false;
    toast.info("That works on a record: choose All to return to the ledger, or p for this issuer");
    return true;
  };

  // The lit record's reason goes to the status bar; the row never grows a line.
  useLitReason(showingSubscriptions ? subscription?.why : current?.why);
  const emailOpen = Boolean(params.threadId) && activePane === "reader";
  usePageKeys(
    emailOpen || ledger.total === 0
      ? []
      : showingSubscriptions
        ? ["archive.ask", "archive.open-email", "archive.issuer", "archive.filter"]
        : [
            "archive.ask",
            "archive.open-email",
            ...(current?.pdf ? ["archive.open-document"] : []),
            "archive.copy-reference",
            "archive.filter",
          ],
  );
  useShortcutScope("archive", !params.threadId || activePane !== "reader");
  useScopeController("archive", {
    down: () => move(1),
    up: () => move(-1),
    ask: () => {
      inputRef.current?.focus();
      inputRef.current?.select();
    },
    copyReference: () => {
      if (!showingSubscriptions) copyRef();
    },
    copyAmount: () => {
      if (showingSubscriptions) copySubscriptionAmount(subscription);
      else if (current) void copyAmount(current);
    },
    openDocument: () => {
      if (showingSubscriptions) {
        setCardOpen(true);
        return;
      }
      if (onAnswer) answerHint.dismiss();
      if (current) void openDocument(current);
    },
    openEmail: () =>
      showingSubscriptions ? openSubscriptionEmail(subscription) : openEmail(current),
    issuer: () => (showingSubscriptions ? subscriptionIssuer(subscription) : issuerPage(current)),
    prevYear: () => (recordOnlyAction() ? undefined : stepTo(-1)),
    nextYear: () => (recordOnlyAction() ? undefined : stepTo(1)),
    edit: () => {
      if (recordOnlyAction()) return;
      if (cardRecord) openMailDialog({ kind: "record-edit", record: cardRecord });
    },
    check: () => {
      if (recordOnlyAction()) return;
      if (current) void markChecked(current);
    },
    dismiss: () => {
      if (recordOnlyAction()) return;
      if (current) void markNotRecord(current);
    },
    export: () => {
      if (recordOnlyAction()) return;
      openMailDialog({ kind: "record-export", account, filter });
    },
    makeTodo: () => {
      if (recordOnlyAction()) return;
      if (!current?.message_id) return;
      openMailDialog({
        kind: "todo-make",
        messageId: current.message_id,
        suggestion: current.warranty_until
          ? `Claim warranty for ${current.title ?? current.issuer ?? "it"}`
          : "",
        subject: current.title ?? undefined,
      });
    },
    filter: () => setFacetsOpen(true),
    close: () => {
      if (cardOpen) setCardOpen(false);
      else if (closeHint) closeHint();
      else if (list) clearSearch();
      else if (showingSubscriptions) setSection("ledger");
      else if (filter.issuer) setFilter({ ...filter, issuer: undefined });
    },
  });

  const groups = useMemo(() => groupByMonth(records, months), [records, months]);
  const position = useMemo(() => new Map(records.map((record, at) => [record.id, at])), [records]);
  const chip = activeChip(filter);

  if (ledger.total === 0) {
    return (
      <PageEmpty
        framed
        glyph="archive"
        title="Nothing filed yet"
        body={
          [ledger.first_run?.line, ledger.empty_state ?? guide?.never_had_any]
            .filter(Boolean)
            .join(" ")
            .replaceAll("`", "") || "Records file here as mxr reads your mail."
        }
      />
    );
  }

  if (singlePane && cardOpen && showingSubscriptions && subscription) {
    return (
      <div className="min-h-0 flex-1 overflow-y-auto pb-6" data-testid="subscription-card-screen">
        <button
          type="button"
          onClick={() => setCardOpen(false)}
          className="mx-3 mt-3 inline-flex min-h-10 items-center gap-1.5 rounded-md px-2 text-sm text-muted-foreground hover:bg-accent"
        >
          <ArrowLeft aria-hidden className="size-4" /> Subscriptions
        </button>
        <SubscriptionCard
          subscription={subscription}
          onCopyAmount={() => copySubscriptionAmount(subscription)}
          onOpenEmail={() => openSubscriptionEmail(subscription)}
          onIssuer={() => {
            setCardOpen(false);
            subscriptionIssuer(subscription);
          }}
        />
      </div>
    );
  }

  if (singlePane && cardOpen && cardRecord && !showingSubscriptions) {
    return (
      <div className="min-h-0 flex-1 overflow-y-auto pb-6" data-testid="record-card-screen">
        <button
          type="button"
          onClick={() => setCardOpen(false)}
          className="mx-3 mt-3 inline-flex min-h-10 items-center gap-1.5 rounded-md px-2 text-sm text-muted-foreground hover:bg-accent"
        >
          <ArrowLeft aria-hidden className="size-4" /> Archive
        </button>
        <RecordCard
          emailButton
          record={cardRecord}
          onCopy={(field) => void copyText(field.copy, field.label.toLowerCase())}
          onOpenDocument={() => void openDocument(cardRecord)}
          onOpenEmail={() => openEmail(cardRecord)}
          onIssuer={() => {
            setCardOpen(false);
            issuerPage(cardRecord);
          }}
        />
      </div>
    );
  }

  return (
    <ModeFrame width="wide" className="flex min-h-0 flex-1">
      <ResizablePanelGroup className="min-h-0 flex-1" {...pane.groupProps}>
        <ResizablePanel
          id="archive-ledger"
          {...pane.otherPanelProps}
          className="flex min-h-0 flex-col"
        >
          <div ref={listRef} className="@container min-h-0 min-w-0 flex-1 overflow-y-auto pb-6">
            <form
              role="search"
              className="mx-5 mt-3"
              onSubmit={(event) => {
                event.preventDefault();
                if (answer && !answer.answer && !answer.fallback?.answer) {
                  onSearchAll();
                  return;
                }
                inputRef.current?.blur();
                // A list's cursor is already on its best match.
                if (!list) setOnAnswer(true);
              }}
            >
              <label htmlFor={inputId} className="sr-only">
                Ask Archive
              </label>
              <div className="relative">
                <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 font-mono text-xs text-muted-foreground">
                  /
                </span>
                <Input
                  id={inputId}
                  ref={inputRef}
                  value={query}
                  data-testid="archive-ask"
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      if (query) setQuery("");
                      else event.currentTarget.blur();
                    }
                  }}
                  placeholder="What are you looking for?"
                  className="h-10 pl-7 text-base"
                  autoComplete="off"
                />
              </div>
            </form>
            {list ? (
              <MatchesHeader list={list} onClear={clearSearch} onIssuer={listIssuerPage} />
            ) : answer ? (
              <>
                <AnswerCard
                  answer={answer}
                  onCopy={(text) => void copyText(text, "answer")}
                  onOpenDocument={(record) => void openDocument(record)}
                  onOpenEmail={(record) => openEmail(record)}
                  onShowAll={onShowAll}
                />
                {answerHint.hint ? (
                  <div className="mx-5">
                    <AnchoredHint hint={answerHint.hint} onDismiss={answerHint.dismiss} />
                  </div>
                ) : null}
              </>
            ) : answerPending ? (
              <p className="mx-5 mt-3 text-xs text-muted-foreground">Looking…</p>
            ) : null}

            {/* A listed query is not narrowed by the ledger's chips and filters. */}
            {list ? null : (
              <>
                {ledger.coming_up.length > 0 ? (
                  <section aria-label="Coming up" data-testid="coming-up" className="mx-5 mt-4">
                    <SectionHeading>Coming up</SectionHeading>
                    <ul className="mt-1 grid gap-0.5 text-sm">
                      {ledger.coming_up.map((moment) => (
                        <li key={`${moment.kind}-${moment.record_id}`}>
                          <button
                            type="button"
                            className="min-h-8 text-left text-foreground hover:underline"
                            onClick={() => {
                              // A subscription's price change or missed charge opens it.
                              if (SUBSCRIPTION_SIGNALS.has(moment.kind) && moment.group_id) {
                                setSection("subscriptions");
                                setSubscriptionId(moment.group_id);
                                if (singlePane) setCardOpen(true);
                                return;
                              }
                              const target = records.find(
                                (record) => record.id === moment.record_id,
                              );
                              if (target) {
                                select(target);
                                if (singlePane) setCardOpen(true);
                              }
                            }}
                          >
                            {moment.label}
                          </button>
                        </li>
                      ))}
                    </ul>
                  </section>
                ) : null}

                <div className="mx-5 mt-4 flex items-center gap-3">
                  <Segmented
                    label="Kind"
                    className="min-w-0 max-w-full overflow-x-auto"
                    value={showingSubscriptions ? SUBSCRIPTIONS_SEGMENT : (chip ?? "")}
                    options={KIND_SEGMENTS}
                    onChange={(id) => {
                      if (id === SUBSCRIPTIONS_SEGMENT) {
                        setSection("subscriptions");
                        setCardOpen(false);
                        return;
                      }
                      const kindChip = KIND_CHIPS.find((candidate) => candidate.id === id);
                      if (kindChip) setFilter({ ...filter, kinds: kindChip.kinds });
                    }}
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    title="Filters (g f)"
                    aria-keyshortcuts="g f"
                    onClick={() => setFacetsOpen(true)}
                    className="type-body ml-auto h-7 shrink-0 gap-1.5 px-2 font-normal text-text-2 hover:bg-ink-lamp hover:text-text-1"
                  >
                    <SlidersHorizontal aria-hidden className="size-4" strokeWidth={1.75} /> Filters
                  </Button>
                </div>
                {showingSubscriptions ? (
                  subscriptions.data ? (
                    <SubscriptionsList
                      data={subscriptions.data}
                      cursorId={subscription?.id ?? null}
                      onSelect={selectSubscription}
                    />
                  ) : subscriptions.isError ? (
                    <p className="mx-5 mt-4 text-sm text-muted-foreground">
                      Couldn't load subscriptions: {subscriptions.error.message}
                    </p>
                  ) : (
                    <p className="mx-5 mt-4 text-sm text-muted-foreground">
                      Finding subscriptions…
                    </p>
                  )
                ) : null}
                {showingSubscriptions ? null : (
                  <ActiveFilters filter={filter} setFilter={setFilter} ledger={ledger} />
                )}

                {!showingSubscriptions && ledger.issuer ? (
                  <section
                    aria-label={`${ledger.issuer.name}'s records`}
                    data-testid="issuer-page"
                    className="mx-5 mt-3"
                  >
                    <h2 className="text-md font-semibold">{ledger.issuer.name}</h2>
                    <p className="text-xs text-muted-foreground">
                      {plural(ledger.issuer.count, "record")}
                      {ledger.issuer.totals.length > 0
                        ? ` · ${ledger.issuer.totals.map((total) => total.display).join(" + ")}`
                        : ""}
                    </p>
                  </section>
                ) : null}

                {!showingSubscriptions && ledger.empty_state && ledger.matching === 0 ? (
                  <p
                    data-testid="records-empty"
                    className="mx-5 mt-4 text-sm text-muted-foreground"
                  >
                    {ledger.empty_state}
                  </p>
                ) : null}
              </>
            )}

            <div className="mt-2" hidden={showingSubscriptions}>
              {groups.map((group, at) => (
                <section
                  key={group.month?.month ?? `undated-${at}`}
                  aria-label={group.month?.label ?? "Undated"}
                  data-testid="record-month"
                >
                  <SectionHeading className="mx-5 mb-1 mt-4 flex items-baseline justify-between gap-3">
                    <span>{group.month?.label ?? "Undated"}</span>
                    {group.month ? (
                      <span
                        data-testid="month-totals"
                        className="tabular-nums normal-case tracking-normal"
                      >
                        {group.month.count} ·{" "}
                        {group.month.totals.length > 0
                          ? group.month.totals.map((total) => total.display).join(" + ")
                          : "no amounts"}
                      </span>
                    ) : null}
                  </SectionHeading>
                  <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
                    {group.records.map((record) => {
                      const rowIndex = position.get(record.id) ?? 0;
                      const row = (
                        <LedgerRow
                          key={record.id}
                          record={record}
                          index={rowIndex}
                          lit={!onAnswer && rowIndex === index}
                          via={via.current}
                          best={record.id === bestId}
                          onSelect={singlePane ? openCard : selectByPointer}
                          onOpen={openEmail}
                          onDocument={openDocumentFor}
                        />
                      );
                      if (rowIndex !== 0 || !recordHint.hint) return row;
                      return (
                        <Fragment key={record.id}>
                          {row}
                          <li className="mx-5">
                            <AnchoredHint hint={recordHint.hint} onDismiss={recordHint.dismiss} />
                          </li>
                        </Fragment>
                      );
                    })}
                  </ul>
                </section>
              ))}
            </div>
            {!showingSubscriptions && list && list.count > list.offset + records.length ? (
              <div className="mx-5 mt-4">
                <Button variant="outline" size="sm" onClick={onMoreMatches}>
                  Show more ({records.length} of {list.count})
                </Button>
              </div>
            ) : null}
            {!showingSubscriptions && !list && ledger.matching > records.length ? (
              <div className="mx-5 mt-4">
                <Button variant="outline" size="sm" onClick={onMore}>
                  Show more ({records.length} of {ledger.matching})
                </Button>
              </div>
            ) : null}
          </div>
        </ResizablePanel>
        {(showSideCard && cardRecord) || showSideSubscription ? (
          <>
            <ResizableHandle aria-label="Resize record card" {...pane.handleProps} />
            <ResizablePanel
              id="archive-card"
              {...pane.sidePanelProps}
              className="flex min-h-0 flex-col"
            >
              {showingSubscriptions && subscription ? (
                <section aria-label="Subscription" className="min-h-0 flex-1 overflow-y-auto">
                  <SubscriptionCard
                    subscription={subscription}
                    onCopyAmount={() => copySubscriptionAmount(subscription)}
                    onOpenEmail={() => openSubscriptionEmail(subscription)}
                    onIssuer={() => subscriptionIssuer(subscription)}
                  />
                </section>
              ) : cardRecord ? (
                <section aria-label="Record" className="min-h-0 flex-1 overflow-y-auto">
                  <RecordCard
                    record={cardRecord}
                    onCopy={(field) => void copyText(field.copy, field.label.toLowerCase())}
                    onOpenDocument={() => void openDocument(cardRecord)}
                    onOpenEmail={() => openEmail(cardRecord)}
                    onIssuer={() => issuerPage(cardRecord)}
                  />
                </section>
              ) : null}
            </ResizablePanel>
          </>
        ) : null}
      </ResizablePanelGroup>
      {facetsOpen ? (
        <FacetsPanel
          ledger={ledger}
          filter={filter}
          tray={singlePane}
          onApply={(next) => {
            setFilter(next);
            setFacetsOpen(false);
          }}
          onClose={() => setFacetsOpen(false)}
        />
      ) : null}
    </ModeFrame>
  );
}

/** What narrows the ledger now, each one click from cleared. */
function ActiveFilters({
  filter,
  setFilter,
  ledger,
}: {
  filter: RecordFilter;
  setFilter: (filter: RecordFilter) => void;
  ledger: RecordLedger;
}) {
  const parts: { label: string; clear: RecordFilter }[] = [];
  if (filter.issuer) parts.push({ label: filter.issuer, clear: { ...filter, issuer: undefined } });
  if (filter.year != null)
    parts.push({ label: String(filter.year), clear: { ...filter, year: undefined } });
  if (filter.has_pdf != null)
    parts.push({
      label: filter.has_pdf ? "Has PDF" : "No PDF",
      clear: { ...filter, has_pdf: undefined },
    });
  if (filter.checked != null)
    parts.push({
      label: filter.checked ? "Checked" : "Unchecked",
      clear: { ...filter, checked: undefined },
    });
  if (filter.min_amount_minor != null || filter.max_amount_minor != null)
    parts.push({
      label: "Amount range",
      clear: { ...filter, min_amount_minor: undefined, max_amount_minor: undefined },
    });
  if (parts.length === 0) return null;
  return (
    <p className="mx-5 mt-2 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
      <span>{plural(ledger.matching, "record")}:</span>
      {parts.map((part) => (
        <button
          key={part.label}
          type="button"
          onClick={() => setFilter(part.clear)}
          aria-label={`Clear ${part.label}`}
          className="inline-flex min-h-7 items-center gap-1 rounded-sm border border-rule px-2 text-text-1 hover:bg-ink-lamp"
        >
          {part.label} <X aria-hidden className="size-3" />
        </button>
      ))}
    </p>
  );
}
