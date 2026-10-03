import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { ChevronRight, ListTodo, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { useModeGuide, useRetireCard, type ModeGuide } from "@/features/modes/api";
import { ModeCard } from "@/features/modes/ModeCard";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { fetchRunway, TODO_KEY, type Todo, type TodoRunway } from "./api";
import { TodoCatchupView, TodoExpiredView } from "./TodoCatchupView";
import { TodoRow } from "./TodoRow";
import { openCount, primaryAction, runwayItems, type RunwayItem } from "./todoRows";
import { markNotTodo, restoreTodos, tickOff, useTodoHidden } from "./todoVerbs";

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

/**
 * To do: things email asked you to do, as a runway of instructions
 * ordered by when to act. Not a list of emails: each row is a task titled
 * verb plus object, with its dates and one button, and the email it came
 * from is one key away (`o`).
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
    <PlaceLayout basePath="/todo" label="To do" threadIds={threadIds}>
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
        <ListSkeleton quiet={phase === "quiet"} />
      ) : status.isError ? (
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't load To do"
          body={status.error.message}
          action={
            <Button size="sm" onClick={() => void status.refetch()}>
              Try again
            </Button>
          }
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
    <header className="shrink-0 border-b border-border px-5 pb-3 pt-4">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className="text-[17px] font-semibold tracking-tight text-foreground">To do</h1>
        <p data-testid="mode-header" className="min-w-0 text-[12.5px] text-muted-foreground">
          {guide?.header ??
            runway?.header ??
            "Things email asked you to do, ordered by when to act."}
        </p>
      </div>
      {runway?.headline ? (
        <p data-testid="todo-headline" className="mt-1.5 text-balance text-[15px] text-foreground">
          {runway.headline}
        </p>
      ) : null}
    </header>
  );
}

function BandHeading({ children, count }: { children: ReactNode; count?: number }) {
  return (
    <h2 className="mx-5 mb-1 mt-4 flex items-baseline gap-2 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
      {children}
      {count !== undefined ? <span className="tabular-nums">{count}</span> : null}
    </h2>
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
      className="inline-flex items-center gap-2 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground hover:text-foreground"
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
  const retire = useRetireCard("todo");
  const [open, setOpen] = useState({ whenever: false, done: false });
  const [cursorId, setCursorId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const items = useMemo(() => runwayItems(runway, open, hidden), [runway, open, hidden]);
  const index = Math.max(
    0,
    items.findIndex((item) => item.todo.id === cursorId),
  );
  const current: RunwayItem | undefined = items[index];
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const hasItems = openCount(runway) > 0;
  const cardShown = Boolean(guide && !guide.card_seen && hasItems);
  // `mutate` is stable; the whole mutation object is not, and the rows are
  // memoized on the callbacks built from this.
  const { mutate: retireMutate } = retire;
  const cardSeen = guide?.card_seen ?? true;
  const retireCard = useCallback(() => {
    if (!cardSeen) retireMutate();
  }, [cardSeen, retireMutate]);

  const openEmail = useCallback(
    (todo: Todo) => {
      if (todo.thread_id) nav?.open(todo.thread_id);
    },
    [nav],
  );
  const runPrimary = useCallback(
    (todo: Todo) => {
      const action = primaryAction(todo);
      if (!action) return;
      // Doing the mode's main verb retires its card: the tip is spent.
      retireCard();
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
          search: {
            link: action.url,
            mid: action.messageId,
            title: todo.title,
            domain: action.domain,
          },
        });
        return;
      }
      openEmail(todo);
    },
    [navigate, openEmail, retireCard, setActivePane],
  );
  const done = useCallback((todo: Todo) => void tickOff(todo), []);
  const restore = useCallback((todo: Todo) => void restoreTodos([todo]), []);
  const select = useCallback((todo: Todo) => setCursorId(todo.id), []);
  const toggleSource = useCallback(
    (todo: Todo) => setExpandedId((id) => (id === todo.id ? null : todo.id)),
    [],
  );

  const move = (delta: number) => {
    const next = items[Math.min(items.length - 1, Math.max(0, index + delta))];
    if (next) setCursorId(next.todo.id);
  };
  const actionable = current && current.band !== "done" ? current.todo : undefined;
  const openView = (view: "catchup" | "expired") =>
    getRuntimeNavigate().navigate(`/todo?view=${view}`);

  // The reader keeps its own keys while it has focus.
  useShortcutScope("todo", !params.threadId || activePane !== "reader");
  useScopeController("todo", {
    down: () => move(1),
    up: () => move(-1),
    primary: () => {
      if (!current) return;
      if (current.band === "done") restore(current.todo);
      else runPrimary(current.todo);
    },
    done: () => actionable && done(actionable),
    dismiss: () => actionable && void markNotTodo([actionable]),
    schedule: () => actionable && openMailDialog({ kind: "todo-schedule", todo: actionable }),
    edit: () => actionable && openMailDialog({ kind: "todo-edit", todo: actionable }),
    source: () => current && toggleSource(current.todo),
    expired: () => openView("expired"),
    catchup: () => openView("catchup"),
    closeCard: cardShown ? retireCard : undefined,
  });

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

  const row = (item: RunwayItem) => {
    const at = position.get(item) ?? 0;
    return (
      <TodoRow
        key={item.todo.id}
        todo={item.todo}
        band={item.band}
        index={at}
        focused={at === index}
        expanded={expandedId === item.todo.id}
        onSelect={select}
        onPrimary={runPrimary}
        onDone={done}
        onRestore={restore}
        onToggleSource={toggleSource}
        onOpenEmail={openEmail}
      />
    );
  };

  if (neverHadAny) {
    return (
      <Centered
        icon={<ListTodo className="size-6" />}
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
      {cardShown && guide ? <ModeCard guide={guide} onClose={retireCard} /> : null}
      {runway.catchup_count > 0 ? (
        <p data-testid="catchup-line" className="mx-5 mt-3 text-[13px] text-foreground/90">
          Catch up: {plural(runway.catchup_count, "thing")} from before mxr sorted your mail might
          still need you.{" "}
          <Link
            to="/todo"
            search={{ view: "catchup" }}
            className="underline decoration-border-strong underline-offset-4 hover:decoration-primary"
          >
            Keep or let go
          </Link>{" "}
          <KeyChip className="h-4 px-1">C</KeyChip>
        </p>
      ) : null}

      <div className="max-w-[64rem]">
        <BandHeading count={nowItems.length}>Now</BandHeading>
        {nowItems.length > 0 ? (
          <ul aria-label="Now" className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
            {nowItems.map(row)}
          </ul>
        ) : (
          <p data-testid="todo-empty" className="mx-5 py-2 text-[13px] text-muted-foreground">
            {runway.empty_state ?? guide?.clear_for_now}
          </p>
        )}

        {groups.length > 0 ? (
          <>
            <BandHeading>Coming up</BandHeading>
            {groups.map((group) => (
              <section key={group.label} aria-label={group.label} data-testid="coming-week">
                <h3 className="mx-5 mt-2 text-[12px] text-muted-foreground">{group.label}</h3>
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

        {expiredOnOpen > 0 ? (
          <p data-testid="expired-line" className="mx-5 mt-5 text-[12.5px] text-muted-foreground">
            {expiredOnOpen} expired since you last looked.{" "}
            <Link
              to="/todo"
              search={{ view: "expired" }}
              className="text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
            >
              See them
            </Link>{" "}
            <KeyChip className="h-4 px-1">E</KeyChip>
          </p>
        ) : null}

        {guide ? <KeyLine guide={guide} /> : null}
      </div>
    </div>
  );
}

/** Keys with their verbs at the point of use, from the mode's own table. */
function KeyLine({ guide }: { guide: ModeGuide }) {
  const keys = guide.keys.filter((key) => key.key !== "t" && key.key !== "?" && key.key !== "u");
  return (
    <p className="mx-5 mt-6 hidden flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground md:flex">
      {keys.map((key) => (
        <span key={key.key} className="inline-flex items-center gap-1">
          <KeyChip>{key.key === "Enter" ? "↵" : key.key}</KeyChip> {key.verb}
        </span>
      ))}
    </p>
  );
}
