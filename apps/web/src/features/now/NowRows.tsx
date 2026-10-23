import { Link } from "@tanstack/react-router";
import { Bell, Newspaper } from "lucide-react";
import { memo, type ReactNode } from "react";

import { Avatar } from "@/components/Avatar";
import { ModeMarks, NewMark } from "@/components/ModeMark";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { rowAge, rowPerson } from "@/features/desk/deskCopy";
import { rowParty } from "@/features/todo/todoRows";
import { otherModes, type ThreadModes } from "@/features/modes/membership";
import { NewSenderChips } from "@/features/modes/NewSenderChips";
import { displaySourceName } from "@/features/updates/digestView";
import { useClockLabel } from "@/lib/minuteClock";
import { definedShortcut } from "@/lib/actions/definition";
import { cn } from "@/lib/utils";

import { itemTarget, sinceLabel, type NowItem } from "./nowItems";

type Person = Extract<NowItem, { kind: "person" }>;
type Due = Extract<NowItem, { kind: "todo" }>;
type Card = Extract<NowItem, { kind: "updates" }>;
type Pick = Extract<NowItem, { kind: "reading" }>;

/** What every Now row is told; the page keeps these stable so a lamp move re-renders two rows. */
interface RowState {
  index: number;
  lit: boolean;
  via: LampVia;
  /** The row is folding away after a pointer done. */
  leaving: boolean;
  onSelect: (item: NowItem, via: LampVia) => void;
  onOpen: (item: NowItem) => void;
  onDone: (item: NowItem, via: LampVia) => void;
  /** The hint anchored to this row, while it shows. */
  hint?: ReactNode;
}

/**
 * The shared shell of Now's rows: the row's text is a real link into the
 * row's own mode (Tab reaches it, Enter follows it, a screen reader names
 * it); a click anywhere else on the row only lights it.
 */
function NowRow({
  item,
  state,
  label,
  lead,
  leadControl,
  primary,
  secondary,
  trail,
  time,
  verbs,
  unread,
}: {
  item: NowItem;
  state: RowState;
  /** The link's accessible name: where it opens and what the row says. */
  label: string;
  lead?: ReactNode;
  leadControl?: boolean;
  primary: ReactNode;
  secondary?: ReactNode;
  trail?: ReactNode;
  time?: ReactNode;
  verbs?: ReactNode;
  unread?: boolean;
}) {
  return (
    <Row
      data-index={state.index}
      data-testid="now-row"
      data-kind={item.kind}
      data-focused={state.lit ? "true" : undefined}
      lit={state.lit}
      via={state.via}
      leaving={state.leaving}
      unread={unread}
      hint={state.hint}
      lead={lead}
      leadControl={leadControl}
      primary={primary}
      secondary={secondary}
      trail={trail}
      time={time}
      verbs={verbs}
      onClick={() => state.onSelect(item, "pointer")}
      renderMain={(content, className) => (
        <Link
          // SAFETY: Now items only ever target static routes, which the router accepts as a `to` string.
          {...(itemTarget(item) as { to: string })}
          data-testid="now-open"
          aria-label={label}
          onFocus={() => state.onSelect(item, "key")}
          onClick={() => state.onSelect(item, "pointer")}
          className={cn(className, "focus-visible:ring-2 focus-visible:ring-signal")}
        >
          {content}
        </Link>
      )}
    />
  );
}

function marksOf(modes: ThreadModes | undefined, here: "messages" | "todo" | "reading") {
  return otherModes(modes, here);
}

export const PersonRow = memo(function PersonRow({
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
  const newSender = item.person.new_sender;
  return (
    <NowRow
      item={item}
      state={state}
      label={`Open in Messages: ${who}, ${subject}. ${item.person.why}`}
      unread={row.unread}
      lead={<Avatar name={who} size="lg" />}
      primary={
        <>
          <span className={cn(row.unread && "font-semibold")} title={row.counterparty_email}>
            {who}
          </span>
          {newSender ? <NewMark className="ml-2" /> : null}
        </>
      }
      secondary={subject || undefined}
      trail={<ModeMarks modes={marksOf(modes, "messages")} />}
      time={
        <time dateTime={row.since} className="whitespace-nowrap tabular-nums">
          {age}
        </time>
      }
      verbs={
        newSender ? (
          <NewSenderChips question={newSender} label={who} />
        ) : (
          <>
            <RowVerb
              keys="r"
              label="reply"
              ariaLabel={`Reply to ${who}`}
              testId="now-reply"
              onClick={() => onReply(item)}
            />
            <RowVerb
              keys="e"
              label="done here"
              ariaLabel="Done in Messages (e)"
              testId="now-done"
              onClick={() => state.onDone(item, "pointer")}
            />
          </>
        )
      }
    />
  );
});

/** True when a to-do is due today or earlier: the one place the cue colour belongs on a date. */
function dueByToday(due: Due["todo"]["todo"]): boolean {
  if (due.overdue) return true;
  const at = due.due_at ?? due.act_by_at;
  if (!at) return false;
  const tomorrow = new Date();
  tomorrow.setHours(24, 0, 0, 0);
  return new Date(at).getTime() < tomorrow.getTime();
}

export const TodoRow = memo(function TodoRow({
  item,
  modes,
  ...state
}: RowState & { item: Due; modes?: ThreadModes }) {
  const todo = item.todo.todo;
  return (
    <NowRow
      item={item}
      state={state}
      label={`Open in To do: ${todo.title}, ${todo.when_label}. ${item.todo.why}`}
      leadControl
      lead={
        <button
          type="button"
          data-testid="now-tick"
          aria-label={`Tick off ${todo.title}`}
          title="Tick off (e)"
          onClick={(event) => {
            event.stopPropagation();
            state.onDone(item, "pointer");
          }}
          className="grid size-4 shrink-0 place-items-center rounded-sm border border-text-3 hover:border-text-1 focus-visible:border-signal"
        />
      }
      primary={
        <>
          <span>{todo.title}</span>
          {todo.amount ? (
            <span className="type-meta ml-2 text-text-2">{todo.amount.display}</span>
          ) : null}
        </>
      }
      secondary={rowParty(todo) || undefined}
      trail={<ModeMarks modes={marksOf(modes, "todo")} />}
      time={
        <span
          data-testid="now-due"
          className={cn("whitespace-nowrap", dueByToday(todo) && "text-cue")}
        >
          {todo.when_label}
        </span>
      }
      verbs={
        <RowVerb
          keys="e"
          label="tick off"
          ariaLabel="Tick off (e)"
          testId="now-done"
          onClick={() => state.onDone(item, "pointer")}
        />
      }
    />
  );
});

/**
 * The latest Updates digest as one row: its headline, and the lines that need
 * a look or changed, run together on the one secondary line.
 */
export const UpdatesRow = memo(function UpdatesRow({
  item,
  onLetGo,
  ...state
}: RowState & { item: Card; onLetGo: () => void }) {
  const card = item.card;
  const lines = card.lines ?? [];
  // Only this count is cue-coloured: names in cue blew the 3% yellow budget.
  const needsLook = lines.filter((line) => line.section === "needs_a_look").length;
  return (
    <NowRow
      item={item}
      state={state}
      label={`Open Updates: ${card.headline || card.line}`}
      lead={<Bell aria-hidden className="size-4 shrink-0 text-text-3" strokeWidth={1.75} />}
      primary={<span data-testid="now-updates-line">{card.headline || card.line}</span>}
      secondary={
        lines.length > 0 ? (
          <>
            {needsLook > 0 ? (
              <span data-testid="now-updates-look" className="text-cue">
                {needsLook} need{needsLook === 1 ? "s" : ""} a look
                {" · "}
              </span>
            ) : null}
            {lines.map((line, at) => (
              <span key={line.id} data-testid="now-updates-item">
                {at > 0 ? " · " : null}
                <span className="text-text-1">{displaySourceName(line.source_name)}</span>:{" "}
                {line.fact}
              </span>
            ))}
            {card.more_line ? ` · ${card.more_line}` : null}
          </>
        ) : (
          card.more_line || undefined
        )
      }
      time={
        <span className="whitespace-nowrap">
          {card.cut_label ? card.cut_label : sinceLabel(card.since)}
        </span>
      }
      verbs={
        <>
          <RowVerb
            keys="g u"
            label="open"
            ariaLabel="Open Updates (g u)"
            testId="now-open-updates"
            onClick={() => state.onOpen(item)}
          />
          <RowVerb
            keys={definedShortcut("now.let-go-digest")}
            label="let go"
            ariaLabel={`Let go of this digest (${definedShortcut("now.let-go-digest")})`}
            testId="now-let-go"
            onClick={onLetGo}
          />
        </>
      }
    />
  );
});

export const ReadingRow = memo(function ReadingRow({
  item,
  modes,
  ...state
}: RowState & { item: Pick; modes?: ThreadModes }) {
  const pick = item.pick;
  const from = pick.sender_name?.trim() || pick.sender_email;
  return (
    <NowRow
      item={item}
      state={state}
      label={`Open in Reading: ${pick.subject}, from ${from}. ${pick.why}`}
      lead={<Newspaper aria-hidden className="size-4 shrink-0 text-text-3" strokeWidth={1.75} />}
      primary={pick.subject}
      secondary={from}
      trail={<ModeMarks modes={marksOf(modes, "reading")} />}
      verbs={
        <RowVerb
          keys="e"
          label="let go"
          ariaLabel="Let go (e)"
          testId="now-done"
          onClick={() => state.onDone(item, "pointer")}
        />
      }
    />
  );
});
