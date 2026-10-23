import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { FinishingMoment } from "@/features/low-tide/FinishingMoment";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { NextPlace } from "@/features/modes/NextPlace";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { ModeFrame } from "@/components/ModeFrame";
import { PageFrame } from "@/components/PageFrame";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { useAdvanceOnRemoval } from "@/hooks/useAdvanceOnRemoval";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { pickSearch, useSearchParam } from "@/hooks/useSearchParam";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useLitReason } from "@/state/litReasonStore";
import { stripKeys } from "@/lib/actions/definition";
import { usePageKeys } from "@/state/pageKeysStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { fetchRunway, TODO_KEY, type Todo, type TodoRunway } from "./api";
import { TodoCatchupView, TodoExpiredView } from "./TodoCatchupView";
import { TodoRow } from "./TodoRow";
import { openCount, primaryAction, runwayItems, type RunwayItem } from "./todoRows";
import { restoreTodos, tickOff, useTodoHidden } from "./todoVerbs";
import type { LampVia } from "@/components/Row";
import { useThreadModesMap } from "@/features/modes/membership";
import { SectionHeading } from "@/components/SectionHeading";
import { listMotions } from "@/features/modes/listMotions";

/**
 * The runway for the account scope. The first fetch on each visit records
 * that To do was opened, so "expired since you last looked" starts again;
 * the count it answered with is kept for the visit, since later refetches
 * count from that moment and say zero.
 */
export function useRunway() {
  const account = useUiPrefs((s) => s.accountScope);
  const scope = account ?? "all";
  // The scopes whose first look this visit has recorded: each account's
  // "last looked" moves only when that account's To do is opened.
  const seen = useRef(new Set<string>());
  const [expired, setExpired] = useState<Record<string, number>>({});
  const runway = useQuery({
    queryKey: [...TODO_KEY, "runway", scope],
    queryFn: async () => {
      const markSeen = !seen.current.has(scope);
      seen.current.add(scope);
      const answer = await fetchRunway(account, markSeen);
      if (markSeen) {
        setExpired((counts) => ({ ...counts, [scope]: answer.expired_since_last_looked }));
      }
      return answer;
    },
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
  return { runway, expiredOnOpen: expired[scope] ?? 0 };
}

/** The list's view, row cursor and open source, kept while an email opens. */
const TODO_LIST_SEARCH = ["view", "row", "source"] as const;

/**
 * To do: things email asked you to do, as a runway of instructions
 * ordered by when to act. Not a list of emails: each row is an action titled
 * verb plus object, with its dates and primary action visible; supporting
 * details and the source email are one key away (`o`).
 */
export function TodoRoute({ view }: { view?: "catchup" | "expired" }) {
  const { runway, expiredOnOpen } = useRunway();
  const threadIds = useCallback(() => {
    const data = runway.data;
    if (!data) return [];
    return runwayItems(data, { whenever: true, done: false })
      .map((item) => item.todo.thread_id)
      .filter((id): id is string => Boolean(id));
  }, [runway.data]);
  return (
    <PlaceLayout basePath="/todo" label="To do" threadIds={threadIds} keepSearch={TODO_LIST_SEARCH}>
      {view === "catchup" ? (
        <TodoCatchupView runway={runway.data} />
      ) : view === "expired" ? (
        <TodoExpiredView />
      ) : (
        <Runway status={runway} expiredOnOpen={expiredOnOpen} />
      )}
    </PlaceLayout>
  );
}

function Runway({
  status,
  expiredOnOpen,
}: {
  status: ReturnType<typeof useRunway>["runway"];
  expiredOnOpen: number;
}) {
  const guide = useModeGuide("todo");
  const phase = useDelayedPending(status.isLoading);
  const data = status.data;
  return (
    <>
      <TodoHeader runway={data} guide={guide.data} />
      {phase !== "ready" ? (
        <PageSkeleton framed shown={phase === "skeleton"} />
      ) : status.isError ? (
        <PageError
          framed
          title="Couldn't load To do"
          error={status.error}
          onRetry={() => void status.refetch()}
        />
      ) : data ? (
        <Bands runway={data} guide={guide.data} expiredOnOpen={expiredOnOpen} />
      ) : null}
    </>
  );
}

/** The mode's name, its job in one line, and the day's headline. */
function TodoHeader({ runway, guide }: { runway?: TodoRunway; guide?: ModeGuide }) {
  return (
    <>
      <PageFrame
        title="To do"
        purpose={
          guide?.header ?? runway?.header ?? "Things email asked you to do, ordered by when to act."
        }
      />
      {runway?.headline ? (
        <ModeFrame>
          <p
            data-testid="todo-headline"
            className="type-heading mx-5 mt-3 text-balance text-text-1"
          >
            {runway.headline}
          </p>
        </ModeFrame>
      ) : null}
    </>
  );
}

function BandHeading({ children, count }: { children: ReactNode; count?: number }) {
  return (
    <SectionHeading className="mx-5 mb-1 mt-4" count={count}>
      {children}
    </SectionHeading>
  );
}

/** "Whenever 2 ›": a band folded to one line until opened. */
function FoldLine({
  label,
  count,
  open,
  onToggle,
  testId,
}: {
  label: string;
  count: number;
  open: boolean;
  onToggle: () => void;
  testId: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      aria-expanded={open}
      onClick={onToggle}
      className="inline-flex items-center gap-2 font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground hover:text-foreground"
    >
      {label} <span className="tabular-nums">{count}</span>
      <ChevronRight aria-hidden className={open ? "size-3 rotate-90" : "size-3"} />
    </button>
  );
}

function Bands({
  runway,
  guide,
  expiredOnOpen,
}: {
  runway: TodoRunway;
  guide?: ModeGuide;
  expiredOnOpen: number;
}) {
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const navigate = useNavigate();
  const hidden = useTodoHidden((s) => s.hidden);
  const leaving = useTodoHidden((s) => s.leaving);
  // Only a pointer fades the lamp (D101): a key never animates.
  const via = useRef<LampVia>("key");
  const [open, setOpen] = useState({ whenever: false, done: false });
  // In the URL, so a copied link lands on the same to-do.
  const [cursorId, setCursorId] = useSearchParam("row");
  const [expandedId, setExpandedId] = useSearchParam("source");
  const items = useMemo(() => runwayItems(runway, open, hidden), [runway, open, hidden]);
  // A link to a to-do in a folded band opens that band, once: a row ticked
  // off later moves to Done without opening it.
  const linkedRow = useRef(cursorId);
  useEffect(() => {
    const linked = linkedRow.current;
    linkedRow.current = null;
    if (!linked || items.some((item) => item.todo.id === linked)) return;
    const band = runwayItems(runway, { whenever: true, done: true }, hidden).find(
      (item) => item.todo.id === linked,
    )?.band;
    if (band === "whenever" || band === "done") setOpen((was) => ({ ...was, [band]: true }));
  }, [hidden, items, runway]);
  // A row folding away after a pointer done is still drawn but no longer counts: the lamp is
  // already on its neighbour while it folds. Found by id, never by position.
  const live = useMemo(() => items.filter((item) => !leaving.has(item.todo.id)), [items, leaving]);
  const index = Math.max(
    0,
    live.findIndex((item) => item.todo.id === cursorId),
  );
  const current: RunwayItem | undefined = live[index];
  // A row ticked off or dismissed leaves: the cursor takes the next one.
  const itemIds = useMemo(() => live.map((item) => item.todo.id), [live]);
  useAdvanceOnRemoval({
    ids: itemIds,
    selectedId: cursorId,
    onAdvance: (next) => setCursorId(next),
    onEmpty: () => setCursorId(null),
  });
  const listRef = useRef<HTMLDivElement>(null);
  const currentId = current?.todo.id;
  useEffect(() => {
    listRef.current?.querySelector(`[data-focused="true"]`)?.scrollIntoView({ block: "nearest" });
  }, [currentId]);
  // The mode marks on each row, from the daemon's membership.
  const markedThreads = useMemo(
    () => items.flatMap((item) => (item.todo.thread_id ? [item.todo.thread_id] : [])),
    [items],
  );
  const memberships = useThreadModesMap(markedThreads).data;

  const hasItems = openCount(runway) > 0;
  // The runway hint sits under the first bar; the catch-up hint under the
  // catch-up line. Only one shows at a time (features/hints).
  const firstRow = items.find((item) => item.band !== "done");
  const runwayHint = useHint("todo", "todo.runway", {
    ready: firstRow !== undefined,
    alone: items.length === 1 && runway.catchup_count === 0,
  });
  const catchupHint = useHint("todo", "todo.catchup", { ready: runway.catchup_count > 0 });
  const { dismiss: dismissRunwayHint } = runwayHint;
  const closeHint = useActiveHintDismiss();

  const openEmail = useCallback(
    (todo: Todo) => {
      // The hint names `e` and `o`; doing either is what it asked for.
      dismissRunwayHint();
      if (todo.thread_id) nav?.open(todo.thread_id);
    },
    [dismissRunwayHint, nav],
  );
  const openResolutionEmail = useCallback(
    (todo: Todo, threadId: string, messageId: string) => {
      setActivePane("reader");
      void navigate({
        to: "/todo/$threadId",
        params: { threadId },
        search: (prev) => ({
          ...pickSearch(prev, TODO_LIST_SEARCH),
          mid: messageId,
          title: todo.title,
        }),
      });
    },
    [navigate, setActivePane],
  );
  const runPrimary = useCallback(
    (todo: Todo) => {
      const action = primaryAction(todo);
      if (!action) return;
      // Doing what the button says is what the runway hint names.
      dismissRunwayHint();
      if (action.kind === "reply") {
        useComposeUi.getState().openCompose(replyIntent(action.messageId, "single"), "overlay");
        return;
      }
      if (action.kind === "email" && todo.thread_id) {
        // The link rides in the URL, so the mark lasts exactly as long as
        // this opening of the email.
        setActivePane("reader");
        void navigate({
          to: "/todo/$threadId",
          params: { threadId: todo.thread_id },
          search: (prev) => ({
            ...pickSearch(prev, TODO_LIST_SEARCH),
            link: action.url,
            mid: action.messageId,
            title: todo.title,
            domain: action.domain,
          }),
        });
        return;
      }
      openEmail(todo);
    },
    [dismissRunwayHint, navigate, openEmail, setActivePane],
  );
  const done = useCallback(
    (todo: Todo, how: LampVia) => {
      dismissRunwayHint();
      via.current = how;
      void tickOff(todo, how);
    },
    [dismissRunwayHint],
  );
  const restore = useCallback((todo: Todo) => void restoreTodos([todo]), []);
  const select = useCallback(
    (todo: Todo) => {
      via.current = "pointer";
      setCursorId(todo.id);
    },
    [setCursorId],
  );
  // The details panel reads the latest open id through a ref so the memoised rows keep one callback.
  const expandedRef = useRef(expandedId);
  expandedRef.current = expandedId;
  const toggleDetails = useCallback(
    (todo: Todo) => setExpandedId(expandedRef.current === todo.id ? null : todo.id),
    [setExpandedId],
  );

  const move = (delta: number) => {
    const next = live[Math.min(live.length - 1, Math.max(0, index + delta))];
    if (!next) return;
    via.current = "key";
    setCursorId(next.todo.id);
  };
  const actionable = current && current.band !== "done" ? current.todo : undefined;
  const openView = (view: "catchup" | "expired") => {
    if (view === "catchup") catchupHint.dismiss();
    getRuntimeNavigate().navigate(`/todo?view=${view}`);
  };

  // The reader keeps its own keys while it has focus.
  useShortcutScope("todo", !params.threadId || activePane !== "reader");
  useScopeController("todo", {
    down: () => move(1),
    up: () => move(-1),
    ...listMotions(move),
    primary: () => {
      if (!current) return;
      if (current.band === "done") restore(current.todo);
      else runPrimary(current.todo);
    },
    done: () => actionable && done(actionable, "key"),
    dismiss: () => actionable && openMailDialog({ kind: "todo-correct", todo: actionable }),
    schedule: () => actionable && openMailDialog({ kind: "todo-schedule", todo: actionable }),
    edit: () => actionable && openMailDialog({ kind: "todo-edit", todo: actionable }),
    openEmail: () => current?.todo.thread_id && openEmail(current.todo),
    details: () => current && toggleDetails(current.todo),
    expired: () => openView("expired"),
    catchup: () => openView("catchup"),
    closeHint,
  });

  // The status bar says why the lit row is here and which keys work on it.
  useLitReason(current?.todo.why);
  // While the open email has focus these keys aren't live, so the strip doesn't offer them.
  const listLive = !params.threadId || activePane !== "reader";
  usePageKeys(current && listLive ? stripKeys(current.band === "done" ? "todo.done" : "todo") : []);

  const { byBand, position } = useMemo(() => {
    const grouped: Record<RunwayItem["band"], RunwayItem[]> = {
      now: [],
      coming: [],
      whenever: [],
      done: [],
    };
    const positions = new Map<RunwayItem, number>();
    items.forEach((item, at) => {
      grouped[item.band].push(item);
      positions.set(item, at);
    });
    return { byBand: grouped, position: positions };
  }, [items]);
  const { now: nowItems, coming: comingItems, whenever: wheneverItems, done: doneItems } = byBand;
  const neverHadAny = !hasItems && runway.done_this_week.length === 0 && runway.catchup_count === 0;
  // To do is put to bed when nothing is due today or earlier (docs/blueprint/27).
  const finished = useLowTide(
    "todo",
    neverHadAny,
    nowItems.some((item) => !leaving.has(item.todo.id)),
    { daily: true },
  );

  const row = (item: RunwayItem) => {
    const at = position.get(item) ?? 0;
    return (
      <TodoRow
        key={item.todo.id}
        todo={item.todo}
        band={item.band}
        index={at}
        lit={item.todo.id === current?.todo.id}
        via={via.current}
        details={expandedId === item.todo.id}
        modes={item.todo.thread_id ? memberships?.get(item.todo.thread_id) : undefined}
        onSelect={select}
        onDone={done}
        onRestore={restore}
        onOpenEmail={openEmail}
        onToggleDetails={toggleDetails}
        onOpenResolution={openResolutionEmail}
        hint={
          item === firstRow && runwayHint.hint ? (
            <AnchoredHint hint={runwayHint.hint} onDismiss={runwayHint.dismiss} />
          ) : undefined
        }
      />
    );
  };

  if (neverHadAny) {
    return (
      <PageEmpty
        framed
        glyph="todo"
        title="Nothing here yet"
        body={[runway.empty_state ?? guide?.never_had_any, guide?.add_one?.replaceAll("`", "")]
          .filter(Boolean)
          .join(" ")}
      />
    );
  }

  // Coming up, by week, with "Later" for what shows up after 30 days.
  const groups: { label: string; items: RunwayItem[] }[] = [];
  for (const item of comingItems) {
    const label = item.group ?? "";
    const group = groups.at(-1);
    if (group && group.label === label) group.items.push(item);
    else groups.push({ label, items: [item] });
  }

  return (
    <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
      <ModeFrame>
        {runway.catchup_count > 0 ? (
          <div className="mx-5 mt-3">
            <p data-testid="catchup-line" className="text-sm text-foreground/90">
              Catch up: {plural(runway.catchup_count, "thing")} from before mxr sorted your mail
              might still need you.{" "}
              <Link
                to="/todo"
                search={{ view: "catchup" }}
                onClick={catchupHint.dismiss}
                className="underline decoration-border-strong underline-offset-4 hover:decoration-primary"
              >
                Keep or let go
              </Link>
            </p>
            {catchupHint.hint ? (
              <AnchoredHint hint={catchupHint.hint} onDismiss={catchupHint.dismiss} />
            ) : null}
          </div>
        ) : null}

        <div>
          <BandHeading count={nowItems.length}>Now</BandHeading>
          {nowItems.length > 0 ? (
            <ul aria-label="Now" className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
              {nowItems.map(row)}
            </ul>
          ) : (
            <div data-testid="todo-empty" className="mx-5 py-2">
              {finished ? (
                <PageEmpty
                  moment={<FinishingMoment />}
                  title={runway.empty_state ?? guide?.clear_for_now ?? "Clear for now."}
                  body={<NextPlace current="todo" />}
                />
              ) : (
                <p className="text-sm text-muted-foreground">
                  {runway.empty_state ?? guide?.clear_for_now}
                </p>
              )}
            </div>
          )}

          {groups.length > 0 ? (
            <>
              <BandHeading>Coming up</BandHeading>
              {groups.map((group) => (
                <section key={group.label} aria-label={group.label} data-testid="coming-week">
                  <h3 className="mx-5 mt-2 text-xs text-muted-foreground">{group.label}</h3>
                  <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">{group.items.map(row)}</ul>
                </section>
              ))}
            </>
          ) : null}

          <div className="mx-5 mt-5 flex flex-wrap gap-x-8 gap-y-2">
            {runway.whenever.length > 0 ? (
              <FoldLine
                label="Whenever"
                count={runway.whenever.length}
                open={open.whenever}
                onToggle={() => setOpen({ ...open, whenever: !open.whenever })}
                testId="whenever-toggle"
              />
            ) : null}
            {runway.done_this_week.length > 0 ? (
              <FoldLine
                label="Done this week"
                count={runway.done_this_week.length}
                open={open.done}
                onToggle={() => setOpen({ ...open, done: !open.done })}
                testId="done-toggle"
              />
            ) : null}
          </div>
          {wheneverItems.length > 0 ? (
            <ul aria-label="Whenever" className="mt-1 grid grid-cols-[minmax(0,1fr)] gap-0.5">
              {wheneverItems.map(row)}
            </ul>
          ) : null}
          {doneItems.length > 0 ? (
            <ul aria-label="Done this week" className="mt-1 grid grid-cols-[minmax(0,1fr)] gap-0.5">
              {doneItems.map(row)}
            </ul>
          ) : null}

          {(runway.might_need_you?.length ?? 0) > 0 ? (
            <details className="mx-5 mt-5" data-testid="might-need-you">
              <summary className="cursor-pointer text-text-2">Might need you · {runway.might_need_you?.length}</summary>
              <ul className="mt-2 grid gap-3">
                {runway.might_need_you?.map((item) => <li key={item.asks.message_id}>
                  <button type="button" className="text-left text-text-1 underline" onClick={() => navigate({ to: "/todo/$threadId", params: { threadId: item.thread_id } })}>{item.subject || "Open the source email"}</button>
                  {item.asks.units.filter((unit) => unit.answer === "not_sure").map((unit) => <blockquote key={unit.unit} className="border-l border-rule pl-3 text-text-2">{unit.quote ?? "Open the source email to review this request."}</blockquote>)}
                </li>)}
              </ul>
            </details>
          ) : null}

          {expiredOnOpen > 0 ? (
            <p data-testid="expired-line" className="mx-5 mt-5 text-xs text-muted-foreground">
              {expiredOnOpen} expired since you last looked.{" "}
              <Link
                to="/todo"
                search={{ view: "expired" }}
                className="text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
              >
                See them
              </Link>
            </p>
          ) : null}
        </div>
      </ModeFrame>
    </div>
  );
}
