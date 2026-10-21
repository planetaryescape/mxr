import { Link, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ModeColumns, ModeFrame } from "@/components/ModeFrame";
import { PageFrame } from "@/components/PageFrame";
import { ArrivalsSection } from "@/features/arrivals/ArrivalsLine";
import { moveCommands, nowMoveSubject } from "@/features/arrivals/moveSubjects";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { FinishingMoment } from "@/features/low-tide/FinishingMoment";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { withDoneHereHint } from "@/features/hints/doneHereHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { useThreadModesMap } from "@/features/modes/membership";
import { NEW_SENDER_NOTE, newSenderCommands } from "@/features/modes/NewSenderChips";
import { doneToast, markModeDone, NONE_HIDDEN, useModeDone } from "@/features/modes/modeDone";
import { useAdvanceOnRemoval } from "@/hooks/useAdvanceOnRemoval";
import { tickOff } from "@/features/todo/todoVerbs";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useSearchParam } from "@/hooks/useSearchParam";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { useScopeController } from "@/lib/keys/controllers";
import type { LampVia } from "@/components/Row";
import { useLitReason } from "@/state/litReasonStore";
import { usePageKeys } from "@/state/pageKeysStore";
import { rowPerson } from "@/features/desk/deskCopy";

import { useNowQuery, type Now } from "./api";
import { LetGoDigestDialog } from "./LetGoDigestDialog";
import {
  itemTarget,
  litIndex,
  nowItems,
  type CardItem,
  type NowItem,
  type PersonItem,
  type PickItem,
  type TodoItem,
} from "./nowItems";
import { PersonRow, ReadingRow, TodoRow, UpdatesRow } from "./NowRows";
import { SectionHeading } from "@/components/SectionHeading";

/**
 * Now: the front page. At most ten things in four fixed sections (People,
 * Due soon, the latest Updates as one card, and an evening Reading pick),
 * capped and worded by the daemon (`GetNow`) so every client shows the same
 * Now. Acting on a row does it in that row's own mode: Enter opens it
 * there, `e` is done here.
 */
/** The "and 8 more in Messages" link, inside the People eyebrow. */
const MORE_LINK = "hover:text-text-1 hover:underline";

/** How long a row folds after a pointer done (`--duration-fast`). */
const FOLD_MS = 120;

/** The lit item's reason for the status bar. The Updates digest has none of its own. */
function reasonOf(item: NowItem | undefined): string | null {
  switch (item?.kind) {
    case "person":
      return item.person.why;
    case "todo":
      return item.todo.why;
    case "reading":
      return item.pick.why;
    default:
      return null;
  }
}

/** The verbs the status strip shows for the lit item (action ids from features/now/actions.ts). */
function keysOf(item: NowItem | undefined): readonly string[] {
  switch (item?.kind) {
    case "person":
      return ["now.open", "now.reply", "now.done", "now.open-email"];
    case "updates":
      return ["now.open", "now.let-go-digest"];
    case "todo":
    case "reading":
      return ["now.open", "now.done", "now.open-email"];
    default:
      return [];
  }
}

export function NowRoute() {
  const now = useNowQuery();
  const guide = useModeGuide("now");
  const phase = useDelayedPending(now.isLoading);
  return (
    <section
      aria-label="Now"
      className="mode-page flex min-h-0 min-w-0 flex-1 flex-col bg-background"
    >
      <NowHeader now={now.data} guide={guide.data} />
      {now.data ? (
        <ModeFrame width="wide">
          <ArrivalsSection nowWaiting={now.data.item_count > 0} />
        </ModeFrame>
      ) : null}
      {phase !== "ready" ? (
        <PageSkeleton framed lead="avatar" shown={phase === "skeleton"} />
      ) : now.isError ? (
        <PageError
          framed
          title="Couldn't load Now"
          error={now.error}
          onRetry={() => void now.refetch()}
        />
      ) : now.data ? (
        <NowBody now={now.data} />
      ) : null}
    </section>
  );
}

/** "Now", its job in one line, and the day's headline. */
function NowHeader({ now, guide }: { now?: Now; guide?: ModeGuide }) {
  const headline = now && now.item_count > 0 ? now.headline : null;
  return (
    <>
      <PageFrame width="wide" title="Now" purpose={guide?.header ?? now?.header} />
      {headline ? (
        <ModeFrame width="wide">
          <p data-testid="now-headline" className="type-heading mx-5 mt-3 text-balance text-text-1">
            {headline}
          </p>
        </ModeFrame>
      ) : null}
    </>
  );
}

function NowBody({ now }: { now: Now }) {
  const navigate = useNavigate();
  const hidden = useModeDone((s) => s.hidden);
  const items = useMemo(() => nowItems(now, hidden), [hidden, now]);
  // Now is put to bed when it has no items (docs/blueprint/27). A Now still
  // on its first run is sorting, not finished.
  const finished = useLowTide("now", !now.first_run.complete, items.length > 0, { daily: true });
  // Every row, done or not: a row done with the pointer stays in the list while it folds.
  const everyItem = useMemo(() => nowItems(now, NONE_HIDDEN), [now]);
  // In the URL, so coming back to Now (or a copied link) lands on the same row.
  const [cursorKey, setCursorKey] = useSearchParam("row");
  const [letGo, setLetGo] = useState(false);
  // The row is found by its id, never by position, so a refetch that reorders
  // the list keeps the lamp on the same row. A row that left falls to the first
  // until `useAdvanceOnRemoval` moves it to the neighbour.
  const index = litIndex(items, cursorKey);
  const current: NowItem | undefined = items[index];
  // Only a pointer fades the lamp or folds a row (D101): a key never animates.
  const via = useRef<LampVia>("key");
  const [folding, setFolding] = useState<ReadonlySet<string>>(new Set());
  const fold = useCallback((key: string) => {
    if (document.documentElement.dataset.motion === "reduced") return;
    setFolding((open) => new Set(open).add(key));
    window.setTimeout(
      () =>
        setFolding((open) => {
          const next = new Set(open);
          next.delete(key);
          return next;
        }),
      FOLD_MS + 20,
    );
  }, []);
  // A row done here leaves: the cursor takes the next one.
  const itemKeys = useMemo(() => items.map((item) => item.key), [items]);
  useAdvanceOnRemoval({
    ids: itemKeys,
    selectedId: cursorKey,
    onAdvance: (next) => setCursorKey(next),
    onEmpty: () => setCursorKey(null),
  });
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-focused="true"]`)?.scrollIntoView({ block: "nearest" });
  }, [current?.key]);

  // The mode marks on each row, from the daemon's membership. Keyed on every
  // row, hidden or not, so a done in flight doesn't refetch it.
  const threadIds = useMemo(
    () => everyItem.flatMap((item) => ("threadId" in item && item.threadId ? [item.threadId] : [])),
    [everyItem],
  );
  const memberships = useThreadModesMap(threadIds).data;

  // "Each row comes from a mode" sits under the first row; letting go of the
  // digest is explained when the cursor reaches its card.
  const firstWhy = items.find((item) => item.kind !== "updates");
  const fromMode = useHint("now", "now.from_mode", {
    ready: firstWhy !== undefined,
    alone: items.length === 1,
  });
  const letGoHint = useHint("now", "updates.let_go", { ready: current?.kind === "updates" });
  const { dismiss: dismissFromMode } = fromMode;
  const { dismiss: dismissLetGo } = letGoHint;
  const closeHint = useActiveHintDismiss();

  const open = useCallback(
    (item: NowItem, options?: { email?: boolean }) => {
      dismissFromMode();
      setCursorKey(item.key);
      void navigate(itemTarget(item, options) as { to: string });
    },
    [dismissFromMode, navigate, setCursorKey],
  );
  const done = useCallback(
    (item: NowItem, how: LampVia) => {
      dismissFromMode();
      via.current = how;
      if (how === "pointer" && item.kind !== "updates") fold(item.key);
      switch (item.kind) {
        case "person":
          void markModeDone("messages", [item.threadId], { message: withDoneHereHint(doneToast) });
          return;
        case "todo":
          void tickOff(item.todo.todo);
          return;
        case "reading":
          void markModeDone("reading", [item.threadId]);
          return;
        case "updates":
          dismissLetGo();
          setLetGo(true);
      }
    },
    [dismissFromMode, dismissLetGo, fold],
  );
  const reply = useCallback((item: NowItem) => {
    if (item.kind !== "person") return;
    useComposeUi
      .getState()
      .openCompose(replyIntent(item.person.row.message_id, "single"), "overlay");
  }, []);
  const select = useCallback(
    (item: NowItem, how: LampVia) => {
      via.current = how;
      setCursorKey(item.key);
    },
    [setCursorKey],
  );
  const openFromRow = useCallback((item: NowItem) => open(item), [open]);
  const letGoDigest = useCallback(() => {
    dismissLetGo();
    setLetGo(true);
  }, [dismissLetGo]);

  const move = (delta: number) => {
    const next = items[Math.min(items.length - 1, Math.max(0, index + delta))];
    if (!next) return;
    via.current = "key";
    setCursorKey(next.key);
  };
  const litPerson = current?.kind === "person" ? current : null;
  const litSender = litPerson?.person.new_sender;
  // The status bar says why the lit row is here; the row itself never grows a line.
  useLitReason(reasonOf(current), litSender ? NEW_SENDER_NOTE : null);
  // The status strip names the verbs of whatever is lit.
  usePageKeys(keysOf(current));
  useShortcutScope("now", true);
  useScopeController("now", {
    down: () => move(1),
    up: () => move(-1),
    open: () => current && open(current),
    openEmail: () => current && current.kind !== "updates" && open(current, { email: true }),
    done: () => current && done(current, "key"),
    reply: () => current && reply(current),
    letGoDigest: now.updates ? letGoDigest : undefined,
    closeHint,
    ...newSenderCommands(litSender, litPerson ? rowPerson(litPerson.person.row) : ""),
    ...moveCommands(() => (current ? nowMoveSubject(current) : null)),
  });

  if (items.length === 0) {
    return <NowEmpty now={now} finished={finished} />;
  }

  const position = new Map(everyItem.map((item, at) => [item.key, at]));
  const liveKeys = new Set(itemKeys);
  const shown = everyItem.filter((item) => liveKeys.has(item.key) || folding.has(item.key));
  const hintFor = (item: NowItem) => {
    const active =
      item.key === firstWhy?.key && fromMode.hint
        ? fromMode
        : item.kind === "updates" && letGoHint.hint
          ? letGoHint
          : null;
    return active?.hint ? (
      <AnchoredHint hint={active.hint} onDismiss={active.dismiss} />
    ) : undefined;
  };
  const rowState = (item: NowItem) => ({
    index: position.get(item.key) ?? 0,
    lit: item.key === current?.key,
    via: via.current,
    leaving: !liveKeys.has(item.key),
    onSelect: select,
    onOpen: openFromRow,
    onDone: done,
    hint: hintFor(item),
  });
  const people = shown.filter((item): item is PersonItem => item.kind === "person");
  const due = shown.filter((item): item is TodoItem => item.kind === "todo");
  const card = shown.find((item): item is CardItem => item.kind === "updates");
  const pick = shown.find((item): item is PickItem => item.kind === "reading");

  return (
    <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
      <ModeFrame width="wide">
        {!now.first_run.complete ? (
          <p data-testid="now-first-run" className="mx-5 mt-3 text-sm text-muted-foreground">
            Sorting your mail, newest first. Now fills in within a few minutes.
          </p>
        ) : null}
        <ModeColumns split={(people.length > 0 || due.length > 0) && Boolean(card || pick)}>
          <div className="min-w-0">
            {people.length > 0 ? (
              <section aria-labelledby="now-people" data-testid="now-section-people">
                <SectionHeading id="now-people" className="mx-5 mb-1 mt-4">
                  People
                  {now.people.more_line ? (
                    <>
                      {" · "}
                      <Link to="/messages" data-testid="now-more" className={MORE_LINK}>
                        {now.people.more_line}
                      </Link>
                    </>
                  ) : null}
                </SectionHeading>
                {now.people.overload_line ? (
                  <p data-testid="now-overload" className="type-body mx-5 mb-1 text-text-2">
                    {now.people.overload_line}
                  </p>
                ) : null}
                <ul className="grid grid-cols-[minmax(0,1fr)]">
                  {people.map((item) => (
                    <PersonRow
                      key={item.key}
                      item={item}
                      modes={memberships?.get(item.threadId)}
                      onReply={reply}
                      {...rowState(item)}
                    />
                  ))}
                </ul>
              </section>
            ) : null}
            {due.length > 0 ? (
              <section aria-labelledby="now-due" data-testid="now-section-due">
                <SectionHeading id="now-due" className="mx-5 mb-1 mt-4">
                  Due soon
                  {now.due_soon.more_line ? (
                    <>
                      {" · "}
                      <Link to="/todo" data-testid="now-more" className={MORE_LINK}>
                        {now.due_soon.more_line}
                      </Link>
                    </>
                  ) : null}
                </SectionHeading>
                <ul className="grid grid-cols-[minmax(0,1fr)]">
                  {due.map((item) => (
                    <TodoRow
                      key={item.key}
                      item={item}
                      modes={item.threadId ? memberships?.get(item.threadId) : undefined}
                      {...rowState(item)}
                    />
                  ))}
                </ul>
              </section>
            ) : null}
          </div>
          <div className="min-w-0">
            {card ? (
              <section aria-labelledby="now-updates" data-testid="now-section-updates">
                <SectionHeading id="now-updates" className="mx-5 mb-1 mt-4">
                  Updates
                </SectionHeading>
                <ul className="grid grid-cols-[minmax(0,1fr)]">
                  <UpdatesRow item={card} onLetGo={letGoDigest} {...rowState(card)} />
                </ul>
              </section>
            ) : null}
            {pick ? (
              <section aria-labelledby="now-reading" data-testid="now-section-reading">
                <SectionHeading id="now-reading" className="mx-5 mb-1 mt-4">
                  For tonight
                </SectionHeading>
                <ul className="grid grid-cols-[minmax(0,1fr)]">
                  <ReadingRow
                    item={pick}
                    modes={memberships?.get(pick.threadId)}
                    {...rowState(pick)}
                  />
                </ul>
              </section>
            ) : null}
          </div>
        </ModeColumns>
        {now.not_now ? (
          <p data-testid="now-not-now" className="type-meta mx-5 mt-5 text-text-3">
            <Link to="/reading" className="hover:text-text-1 hover:underline">
              {now.not_now}
            </Link>
          </p>
        ) : null}
      </ModeFrame>
      {now.updates ? (
        <LetGoDigestDialog card={now.updates} open={letGo} onOpenChange={setLetGo} />
      ) : null}
    </div>
  );
}

/**
 * Nothing on Now. Before the first run finishes, the never-had-any line;
 * after it, the empty state with the low tide scene as its finishing moment.
 */
function NowEmpty({ now, finished }: { now: Now; finished: boolean }) {
  if (!now.first_run.complete) {
    return (
      <PageEmpty framed glyph="now" title="Sorting your mail" body={now.empty_state ?? undefined} />
    );
  }
  return (
    <PageEmpty
      framed
      glyph={finished ? undefined : "now"}
      moment={finished ? <FinishingMoment /> : undefined}
      title={now.empty_state ?? "Clear."}
      body={
        <>
          New mail still arrives in the inbox: <kbd className="font-mono">g</kbd>{" "}
          <kbd className="font-mono">i</kbd>.
        </>
      }
    />
  );
}
