import { Link, useNavigate } from "@tanstack/react-router";
import { RefreshCw, Sun } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { LowTide } from "@/features/low-tide/LowTide";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useModeGuide, useRetireCard, type ModeGuide } from "@/features/modes/api";
import { useThreadModesMap } from "@/features/modes/membership";
import { ModeCard } from "@/features/modes/ModeCard";
import { markModeDone, NONE_HIDDEN, useModeDone } from "@/features/modes/modeDone";
import { tickOff } from "@/features/todo/todoVerbs";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { useScopeController } from "@/lib/keys/controllers";

import { useNowQuery, type Now } from "./api";
import { LetGoDigestDialog } from "./LetGoDigestDialog";
import {
  itemPath,
  nowItems,
  type CardItem,
  type NowItem,
  type PersonItem,
  type PickItem,
  type TodoItem,
} from "./nowItems";
import { PersonRow, ReadingRow, SectionHeading, TodoRow, UpdatesCard } from "./NowRows";

/**
 * Now: the front page. At most ten things in four fixed sections (People,
 * Due soon, the latest Updates as one card, and an evening Reading pick),
 * capped and worded by the daemon (`GetNow`) so every client shows the same
 * Now. Acting on a row does it in that row's own mode: Enter opens it
 * there, `e` is done here.
 */
export function NowRoute() {
  const now = useNowQuery();
  const guide = useModeGuide("now");
  const phase = useDelayedPending(now.isLoading);
  return (
    <section aria-label="Now" className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
      <NowHeader now={now.data} guide={guide.data} />
      {phase !== "ready" ? (
        <ListSkeleton quiet={phase === "quiet"} />
      ) : now.isError ? (
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't load Now"
          body={now.error.message}
          action={
            <Button size="sm" onClick={() => void now.refetch()}>
              Try again
            </Button>
          }
        />
      ) : now.data ? (
        <NowBody now={now.data} guide={guide.data} />
      ) : null}
    </section>
  );
}

/** "Now", its job in one line, and the day's headline. */
function NowHeader({ now, guide }: { now?: Now; guide?: ModeGuide }) {
  return (
    <header className="shrink-0 border-b border-border px-5 pb-3 pt-4">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className="text-[17px] font-semibold tracking-tight text-foreground">Now</h1>
        <p data-testid="mode-header" className="min-w-0 text-[12.5px] text-muted-foreground">
          {guide?.header ?? now?.header}
        </p>
      </div>
      {now && now.item_count > 0 ? (
        <p data-testid="now-headline" className="mt-1.5 text-balance text-[15px] text-foreground">
          {now.headline}
        </p>
      ) : null}
    </header>
  );
}

function NowBody({ now, guide }: { now: Now; guide?: ModeGuide }) {
  const navigate = useNavigate();
  const hidden = useModeDone((s) => s.hidden);
  const items = useMemo(() => nowItems(now, hidden), [hidden, now]);
  const [cursorKey, setCursorKey] = useState<string | null>(null);
  const [letGo, setLetGo] = useState(false);
  const index = Math.max(
    0,
    items.findIndex((item) => item.key === cursorKey),
  );
  const current: NowItem | undefined = items[index];
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  // "Also in To do: …" on each row, from the daemon's membership. Keyed
  // on every row, hidden or not, so a done in flight doesn't refetch it.
  const threadIds = useMemo(
    () =>
      nowItems(now, NONE_HIDDEN).flatMap((item) =>
        "threadId" in item && item.threadId ? [item.threadId] : [],
      ),
    [now],
  );
  const memberships = useThreadModesMap(threadIds).data;

  const retire = useRetireCard("now");
  const { mutate: retireMutate } = retire;
  const cardSeen = guide?.card_seen ?? true;
  const cardShown = Boolean(guide && !cardSeen && now.item_count > 0);
  const retireCard = useCallback(() => {
    if (!cardSeen) retireMutate();
  }, [cardSeen, retireMutate]);

  const open = useCallback(
    (item: NowItem) => {
      setCursorKey(item.key);
      void navigate({ to: itemPath(item) });
    },
    [navigate],
  );
  const done = useCallback(
    (item: NowItem) => {
      // Done here is Now's main verb: using it retires the card about Now.
      retireCard();
      switch (item.kind) {
        case "person":
          void markModeDone("messages", [item.threadId]);
          return;
        case "todo":
          void tickOff(item.todo.todo);
          return;
        case "reading":
          void markModeDone("reading", [item.threadId]);
          return;
        case "updates":
          setLetGo(true);
      }
    },
    [retireCard],
  );
  const reply = useCallback((item: NowItem) => {
    if (item.kind !== "person") return;
    useComposeUi
      .getState()
      .openCompose(replyIntent(item.person.row.message_id, "single"), "overlay");
  }, []);
  const select = useCallback((item: NowItem) => setCursorKey(item.key), []);

  const move = (delta: number) => {
    const next = items[Math.min(items.length - 1, Math.max(0, index + delta))];
    if (next) setCursorKey(next.key);
  };
  useShortcutScope("now", true);
  useScopeController("now", {
    down: () => move(1),
    up: () => move(-1),
    open: () => current && open(current),
    openEmail: () => current && current.kind !== "updates" && open(current),
    done: () => current && done(current),
    reply: () => current && reply(current),
    letGoDigest: now.updates ? () => setLetGo(true) : undefined,
    closeCard: cardShown ? retireCard : undefined,
  });

  if (items.length === 0) {
    return <NowEmpty now={now} />;
  }

  const position = new Map(items.map((item, at) => [item.key, at]));
  const rowState = (item: NowItem) => ({
    index: position.get(item.key) ?? 0,
    focused: (position.get(item.key) ?? -1) === index,
    onSelect: select,
    onDone: done,
  });
  const people = items.filter((item): item is PersonItem => item.kind === "person");
  const due = items.filter((item): item is TodoItem => item.kind === "todo");
  const card = items.find((item): item is CardItem => item.kind === "updates");
  const pick = items.find((item): item is PickItem => item.kind === "reading");

  return (
    <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
      {cardShown && guide ? <ModeCard guide={guide} onClose={retireCard} /> : null}
      {!now.first_run.complete ? (
        <p data-testid="now-first-run" className="mx-5 mt-3 text-[13px] text-muted-foreground">
          Sorting your mail, newest first. Now fills in within a few minutes.
        </p>
      ) : null}
      <div className="max-w-[64rem]">
        {people.length > 0 ? (
          <section aria-labelledby="now-people" data-testid="now-section-people">
            <SectionHeading id="now-people" more={now.people.more_line} to="/messages">
              People
            </SectionHeading>
            {now.people.overload_line ? (
              <p data-testid="now-overload" className="mx-5 mb-1 text-[13px] text-foreground/90">
                {now.people.overload_line}
              </p>
            ) : null}
            <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
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
            <SectionHeading id="now-due" more={now.due_soon.more_line} to="/todo">
              Due soon
            </SectionHeading>
            <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
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
        {card ? (
          <UpdatesCard item={card} onLetGo={() => setLetGo(true)} {...rowState(card)} />
        ) : null}
        {pick ? (
          <section aria-labelledby="now-reading" data-testid="now-section-reading">
            <SectionHeading id="now-reading" to="/reading">
              For tonight
            </SectionHeading>
            <ul className="grid grid-cols-[minmax(0,1fr)]">
              <ReadingRow item={pick} modes={memberships?.get(pick.threadId)} {...rowState(pick)} />
            </ul>
          </section>
        ) : null}
        {now.not_now ? (
          <p data-testid="now-not-now" className="mx-5 mt-5 text-[12.5px] text-muted-foreground">
            <Link to="/reading" className="hover:text-foreground hover:underline">
              {now.not_now}
            </Link>
          </p>
        ) : null}
        {guide ? <KeyLine guide={guide} /> : null}
      </div>
      {now.updates ? (
        <LetGoDigestDialog card={now.updates} open={letGo} onOpenChange={setLetGo} />
      ) : null}
    </div>
  );
}

/**
 * Nothing on Now. Before the first run finishes, the never-had-any line;
 * after it, the low tide scene with when the next thing arrives.
 */
function NowEmpty({ now }: { now: Now }) {
  if (!now.first_run.complete) {
    return (
      <Centered
        icon={<Sun className="size-6" />}
        title="Sorting your mail"
        body={now.empty_state ?? undefined}
      />
    );
  }
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-12">
      <LowTide line={now.empty_state ?? "Clear."} sound className="w-full">
        <p className="mt-3">
          New mail still arrives in the inbox: <kbd className="font-mono">g</kbd>{" "}
          <kbd className="font-mono">i</kbd>.
        </p>
      </LowTide>
    </div>
  );
}

/** Keys with their verbs at the point of use, from Now's own guide. */
function KeyLine({ guide }: { guide: ModeGuide }) {
  const keys = guide.keys.filter((key) => key.key !== "?" && key.key !== "u" && key.key !== "t");
  return (
    <p className="mx-5 mt-6 hidden flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground md:flex">
      {keys.map((key) => (
        <span key={key.key} className="inline-flex items-center gap-1">
          <KeyChip>{key.key === "Enter" ? "↵" : key.key}</KeyChip> {key.verb}
        </span>
      ))}
      <span className="inline-flex items-center gap-1">
        <KeyChip>A</KeyChip> let go of the digest
      </span>
    </p>
  );
}
