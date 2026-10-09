import { Link } from "@tanstack/react-router";
import { Check, Reply } from "lucide-react";
import type { ReactNode } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { rowAge, rowPerson } from "@/features/desk/deskCopy";
import { AlsoInLine } from "@/features/modes/AlsoInLine";
import type { ThreadModes } from "@/features/modes/membership";
import { NewSenderQuestion } from "@/features/modes/NewSenderQuestion";
import { cn } from "@/lib/utils";
import { useClockLabel } from "@/lib/minuteClock";

import { itemPath, sinceLabel, type NowItem } from "./nowItems";

type Person = Extract<NowItem, { kind: "person" }>;
type Due = Extract<NowItem, { kind: "todo" }>;
type Card = Extract<NowItem, { kind: "updates" }>;
type Pick = Extract<NowItem, { kind: "reading" }>;

interface RowState {
  index: number;
  focused: boolean;
  onSelect: (item: NowItem) => void;
  onDone: (item: NowItem) => void;
  /** The hint anchored to this row, while it shows. */
  hint?: ReactNode;
}

/** "PEOPLE                      and 8 more in Messages". */
export function SectionHeading({
  id,
  more,
  to,
  children,
}: {
  id: string;
  more?: string | null;
  to: string;
  children: ReactNode;
}) {
  return (
    <div className="mx-5 mb-1 mt-4 flex items-baseline justify-between gap-3">
      <h2
        id={id}
        className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground"
      >
        {children}
      </h2>
      {more ? (
        <Link
          to={to}
          data-testid="now-more"
          className="text-[12px] text-muted-foreground hover:text-foreground hover:underline"
        >
          {more}
        </Link>
      ) : null}
    </div>
  );
}

/**
 * The row frame every Now item shares: the cursor bar, the row's main text
 * as a real link into the row's own mode (Tab reaches it, Enter follows
 * it, a screen reader names it), its own controls, and lines under it
 * that carry links or buttons of their own.
 */
function RowFrame({
  item,
  state,
  label,
  main,
  meta,
  actions,
  extra,
}: {
  item: NowItem;
  state: RowState;
  /** The link's accessible name: where it opens and what the row says. */
  label: string;
  main: ReactNode;
  /** The row's date or due line, just left of the action rail. */
  meta?: ReactNode;
  actions: ReactNode;
  extra?: ReactNode;
}) {
  return (
    <li
      data-index={state.index}
      data-testid="now-row"
      data-kind={item.kind}
      data-focused={state.focused ? "true" : undefined}
      onMouseEnter={() => state.onSelect(item)}
      className={cn(
        "group/now relative mx-2 grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-0.5 rounded-md px-3 py-2",
        state.focused ? "bg-accent" : "hover:bg-accent/40",
      )}
    >
      {state.focused ? (
        <span aria-hidden className="absolute inset-y-1.5 left-0 w-[2px] rounded-full bg-primary" />
      ) : null}
      <Link
        to={itemPath(item)}
        data-testid="now-open"
        aria-label={label}
        onFocus={() => state.onSelect(item)}
        onClick={() => state.onSelect(item)}
        className="block min-w-0 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {main}
      </Link>
      {/*
        The action rail has one width on every row, buttons shown or not, so
        dates line up in one column against the right edge.
      */}
      <span className="row-span-2 flex items-start gap-1">
        {meta}
        <span className="flex w-[3.75rem] shrink-0 items-start justify-end gap-1">{actions}</span>
      </span>
      {extra ? <div className="min-w-0">{extra}</div> : null}
      {state.hint ? <div className="col-span-2 min-w-0">{state.hint}</div> : null}
    </li>
  );
}

/** The check that does `e`: done here, in the row's own mode. */
function DoneButton({ item, state, title }: { item: NowItem; state: RowState; title: string }) {
  return (
    <button
      type="button"
      data-testid="now-done"
      aria-label={title}
      title={title}
      onClick={(event) => {
        event.stopPropagation();
        state.onDone(item);
      }}
      className={cn(
        "-my-1 grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-primary-muted hover:text-primary",
        state.focused
          ? "opacity-100"
          : "pointer-fine:opacity-0 pointer-fine:group-hover/now:opacity-100",
      )}
    >
      <Check className="size-3.5" strokeWidth={2.25} />
    </button>
  );
}

export function PersonRow({
  item,
  modes,
  onReply,
  ...state
}: RowState & {
  item: Person;
  modes?: ThreadModes;
  onReply: (item: NowItem) => void;
}) {
  const row = item.person.row;
  const who = rowPerson(row);
  const age = useClockLabel((now) => rowAge(row, now).label);
  const subject = row.subject.trim();
  return (
    <RowFrame
      item={item}
      state={state}
      label={`Open in Messages: ${who}, ${subject}. ${item.person.why}`}
      main={
        <>
          <p className="flex min-w-0 items-baseline gap-2 text-[13px]">
            <span
              className={cn("shrink-0", row.unread ? "font-semibold" : "text-foreground/90")}
              title={row.counterparty_email}
            >
              {who}
            </span>
            {subject ? (
              <span className="min-w-0 truncate text-muted-foreground">{subject}</span>
            ) : null}
          </p>
          <p data-testid="now-why" className="truncate text-[12px] text-fg-2">
            {item.person.why}
          </p>
        </>
      }
      extra={
        <>
          {item.person.new_sender ? (
            <NewSenderQuestion question={item.person.new_sender} label={who} />
          ) : null}
          <AlsoInLine modes={modes} here="messages" keys={false} />
        </>
      }
      meta={
        <time
          dateTime={row.since}
          className="whitespace-nowrap font-mono text-2xs tabular-nums text-muted-foreground"
        >
          {age}
        </time>
      }
      actions={
        <>
          <button
            type="button"
            aria-label={`Reply to ${who}`}
            title="Reply (r)"
            onClick={(event) => {
              event.stopPropagation();
              onReply(item);
            }}
            className={cn(
              "-my-1 grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground",
              state.focused
                ? "opacity-100"
                : "pointer-fine:opacity-0 pointer-fine:group-hover/now:opacity-100",
            )}
          >
            <Reply className="size-3.5" />
          </button>
          <DoneButton item={item} state={state} title="Done in Messages (e)" />
        </>
      }
    />
  );
}

export function TodoRow({ item, modes, ...state }: RowState & { item: Due; modes?: ThreadModes }) {
  const todo = item.todo.todo;
  return (
    <RowFrame
      item={item}
      state={state}
      label={`Open in To do: ${todo.title}, ${todo.when_label}. ${item.todo.why}`}
      main={
        <>
          <p className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-[13px]">
            <span className="min-w-0 font-medium text-foreground/90">{todo.title}</span>
            {todo.amount ? (
              <span className="font-mono text-[12px] tabular-nums text-foreground/80">
                {todo.amount.display}
              </span>
            ) : null}
            {todo.person_label ? (
              <span className="text-[12px] text-muted-foreground">{todo.person_label}</span>
            ) : null}
          </p>
          <p data-testid="now-why" className="truncate text-[12px] text-fg-2">
            {item.todo.why}
          </p>
        </>
      }
      extra={<AlsoInLine modes={modes} here="todo" keys={false} />}
      meta={
        <span
          className={cn(
            "whitespace-nowrap text-[12px]",
            todo.overdue ? "text-warning" : "text-muted-foreground",
          )}
        >
          {todo.when_label}
        </span>
      }
      actions={<DoneButton item={item} state={state} title="Tick off (e)" />}
    />
  );
}

/**
 * The latest Updates digest as one card: its headline, up to three lines
 * that need a look or changed, and how much routine waits behind them.
 */
export function UpdatesCard({
  item,
  onLetGo,
  ...state
}: RowState & { item: Card; onLetGo: () => void }) {
  const card = item.card;
  return (
    <section aria-labelledby="now-updates" data-testid="now-section-updates" className="mt-4">
      <div className="mx-5 mb-1 flex items-baseline gap-2">
        <h2
          id="now-updates"
          className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground"
        >
          Updates
        </h2>
        <span className="text-[12px] text-muted-foreground">
          {card.cut_label ? `${card.cut_label} digest` : sinceLabel(card.since)}
        </span>
      </div>
      <ul className="grid grid-cols-[minmax(0,1fr)]">
        <li
          data-index={state.index}
          data-testid="now-row"
          data-kind={item.kind}
          data-focused={state.focused ? "true" : undefined}
          onMouseEnter={() => state.onSelect(item)}
          className={cn(
            "relative mx-2 rounded-md px-3 py-2",
            state.focused ? "bg-accent" : "hover:bg-accent/40",
          )}
        >
          {state.focused ? (
            <span
              aria-hidden
              className="absolute inset-y-1.5 left-0 w-[2px] rounded-full bg-primary"
            />
          ) : null}
          <div className="min-w-0">
            <p data-testid="now-updates-line" className="text-[13px] text-foreground/90">
              {card.headline || card.line}
            </p>
            {card.lines && card.lines.length > 0 ? (
              <ul className="mt-1 grid gap-0.5">
                {card.lines.map((line) => (
                  <li
                    key={line.id}
                    data-testid="now-updates-item"
                    className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-[12.5px]"
                  >
                    <span
                      aria-hidden
                      className={cn(
                        "w-2 shrink-0 font-semibold",
                        line.section === "needs_a_look" ? "text-warning" : "text-primary",
                      )}
                    >
                      {line.section === "needs_a_look" ? "!" : "·"}
                    </span>
                    <span className="shrink-0 font-medium text-foreground/90">
                      {line.source_name}
                    </span>
                    <span className="min-w-0 break-words text-foreground/80">{line.fact}</span>
                    {line.in_todo ? (
                      <span className="text-[11.5px] text-muted-foreground">({line.in_todo})</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : null}
            {card.more_line ? (
              <p className="mt-1 text-[12px] text-muted-foreground">{card.more_line}</p>
            ) : null}
            <p className="mt-1.5 flex flex-wrap items-center gap-2">
              <Button asChild size="sm" variant="outline">
                <Link to="/updates" onFocus={() => state.onSelect(item)}>
                  Open <KeyChip className="h-4 px-1">g u</KeyChip>
                </Link>
              </Button>
              <Button size="sm" variant="ghost" onClick={onLetGo}>
                Let go of this digest <KeyChip className="h-4 px-1">A</KeyChip>
              </Button>
            </p>
            {state.hint}
          </div>
        </li>
      </ul>
    </section>
  );
}

export function ReadingRow({
  item,
  modes,
  ...state
}: RowState & { item: Pick; modes?: ThreadModes }) {
  const pick = item.pick;
  const from = pick.sender_name?.trim() || pick.sender_email;
  return (
    <RowFrame
      item={item}
      state={state}
      label={`Open in Reading: ${pick.subject}, from ${from}. ${pick.why}`}
      main={
        <>
          <p className="flex min-w-0 items-baseline gap-2 text-[13px]">
            <span className="min-w-0 truncate font-medium text-foreground/90">{pick.subject}</span>
            <span className="shrink-0 text-[12px] text-muted-foreground">{from}</span>
          </p>
          <p data-testid="now-why" className="truncate text-[12px] text-fg-2">
            {pick.why}
          </p>
        </>
      }
      extra={<AlsoInLine modes={modes} here="reading" keys={false} />}
      actions={<DoneButton item={item} state={state} title="Let go (e)" />}
    />
  );
}
