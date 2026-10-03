import { Link } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { useCatchupQuery, useExpiredQuery, type Todo, type TodoRunway } from "./api";
import { openCount, rowParty } from "./todoRows";
import { decideCatchup, restoreTodos, useTodoHidden } from "./todoVerbs";

function useCursor(rows: readonly Todo[]) {
  const [cursorId, setCursorId] = useState<string | null>(null);
  const index = Math.max(
    0,
    rows.findIndex((todo) => todo.id === cursorId),
  );
  const move = (delta: number) => {
    const next = rows[Math.min(rows.length - 1, Math.max(0, index + delta))];
    if (next) setCursorId(next.id);
  };
  return { index, current: rows[index], move, setCursorId };
}

function useScrollIntoView(index: number) {
  const ref = useRef<HTMLUListElement>(null);
  useEffect(() => {
    ref.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);
  return ref;
}

function BackLink() {
  return (
    <Link to="/todo" className="text-[12px] text-muted-foreground hover:text-foreground">
      Back to To do <KeyChip className="ml-1 h-4 px-1">Esc</KeyChip>
    </Link>
  );
}

/** A catch-up or Expired row: what it is, who for, and its dates. */
function PlainRow({
  todo,
  index,
  focused,
  onSelect,
  children,
}: {
  todo: Todo;
  index: number;
  focused: boolean;
  onSelect: () => void;
  children: React.ReactNode;
}) {
  const leaving = useTodoHidden((s) => s.leaving.has(todo.id));
  const hide = useTodoHidden((s) => s.hide);
  const party = rowParty(todo);
  return (
    <li
      data-index={index}
      data-testid="todo-row"
      data-id={todo.id}
      aria-current={focused ? "true" : undefined}
      onClick={onSelect}
      onAnimationEnd={leaving ? () => hide([todo.id]) : undefined}
      className={cn(
        "relative mx-2 flex flex-col gap-2 rounded-md px-3 py-2.5 @2xl:flex-row @2xl:items-center @2xl:gap-4",
        focused ? "bg-accent" : "hover:bg-accent/40",
        leaving && "todo-fold pointer-events-none",
      )}
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-2 left-0 w-[2px] rounded-full bg-primary" />
      ) : null}
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-x-3">
          <span className="text-[14px] font-medium text-foreground">{todo.title}</span>
          {party ? <span className="text-[13px] text-muted-foreground">{party}</span> : null}
          {todo.amount ? (
            <span className="font-mono text-[13px] tabular-nums">{todo.amount.display}</span>
          ) : null}
        </div>
        <p className="mt-0.5 font-mono text-2xs text-foreground/80">{todo.when_label}</p>
        <p className="mt-0.5 text-[12px] text-muted-foreground">{todo.why}</p>
      </div>
      <div className="flex shrink-0 gap-2">{children}</div>
    </li>
  );
}

/**
 * The first run's one-time catch-up. The summary says what the first run
 * found; each row from before mxr sorted your mail is kept (it joins the
 * runway) or let go (it joins the Expired list), and "Let go of all"
 * previews the daemon's dry run first.
 */
export function TodoCatchupView({ runway }: { runway: TodoRunway | undefined }) {
  const account = useUiPrefs((s) => s.accountScope);
  const catchup = useCatchupQuery();
  const hidden = useTodoHidden((s) => s.hidden);
  const rows = (catchup.data?.todos ?? []).filter((todo) => !hidden.has(todo.id));
  const { index, current, move, setCursorId } = useCursor(rows);
  const listRef = useScrollIntoView(index);

  useShortcutScope("catchup");
  useScopeController("catchup", {
    down: () => move(1),
    up: () => move(-1),
    keep: () => current && void decideCatchup(account, [current], "keep"),
    letGo: () => current && void decideCatchup(account, [current], "let_go"),
    letGoAll: () => rows.length > 0 && openMailDialog({ kind: "todo-let-go-all", account }),
    back: () => getRuntimeNavigate().navigate("/todo"),
  });

  const open = runway ? openCount(runway) : 0;
  const now = runway?.now.length ?? 0;
  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-testid="todo-catchup">
      <section
        aria-label="Your last two weeks, sorted"
        className="mx-5 mt-4 max-w-[64rem] border-l-2 border-primary bg-surface px-4 py-3"
      >
        <h2 className="text-[15px] font-semibold">Your last two weeks, sorted</h2>
        <p className="mt-1 grid grid-cols-[5rem_minmax(0,1fr)] gap-x-3 text-[13px] @2xl:grid-cols-[5rem_minmax(0,16rem)_minmax(0,1fr)]">
          <span className="font-medium">To do</span>
          <span data-testid="first-run-counts">
            {plural(open, "thing")}, {now} to act on now
          </span>
          <span className="col-span-2 text-muted-foreground @2xl:col-span-1">
            What email asked you to do
          </span>
        </p>
        {catchup.data?.already_over_line ? (
          <p className="mt-1 text-[12.5px] text-muted-foreground">
            {catchup.data.already_over_line}
          </p>
        ) : null}
      </section>
      <header className="mx-5 mt-5 flex max-w-[64rem] flex-wrap items-baseline gap-x-4 gap-y-2">
        <div className="min-w-0 flex-1">
          <h2 className="text-[15px] font-semibold" data-testid="catchup-title">
            {catchup.data?.title ?? "Catch up"}
          </h2>
          {catchup.data && catchup.data.todos.length > 0 ? (
            <p className="mt-0.5 max-w-[62ch] text-[12.5px] text-muted-foreground">
              {catchup.data.why}
            </p>
          ) : null}
        </div>
        {rows.length > 0 ? (
          <Button
            variant="outline"
            size="xs"
            onClick={() => openMailDialog({ kind: "todo-let-go-all", account })}
          >
            Let go of all <KeyChip className="ml-1 h-4 px-1">A</KeyChip>
          </Button>
        ) : null}
        <BackLink />
      </header>
      {catchup.isError ? (
        <p className="mx-5 mt-4 text-[13px] text-muted-foreground">
          Couldn't load the catch-up: {catchup.error.message}
        </p>
      ) : null}
      <ul ref={listRef} aria-label="Catch-up" className="max-w-[64rem] py-2">
        {rows.map((todo, position) => (
          <PlainRow
            key={todo.id}
            todo={todo}
            index={position}
            focused={position === index}
            onSelect={() => setCursorId(todo.id)}
          >
            <Button
              size="xs"
              variant="outline"
              onClick={(event) => {
                event.stopPropagation();
                void decideCatchup(account, [todo], "keep");
              }}
            >
              Keep {position === index ? <KeyChip className="ml-1 h-4 px-1">↵</KeyChip> : null}
            </Button>
            <Button
              size="xs"
              variant="ghost"
              onClick={(event) => {
                event.stopPropagation();
                void decideCatchup(account, [todo], "let_go");
              }}
            >
              Let go {position === index ? <KeyChip className="ml-1 h-4 px-1">e</KeyChip> : null}
            </Button>
          </PlainRow>
        ))}
      </ul>
      {catchup.data && catchup.data.overflow_count > 0 ? (
        <p className="mx-5 pb-4 text-[12.5px] text-muted-foreground">
          {plural(catchup.data.overflow_count, "more")} didn't fit and{" "}
          <Link to="/todo" search={{ view: "expired" }} className="underline underline-offset-4">
            are in the Expired list
          </Link>
          .
        </p>
      ) : null}
    </div>
  );
}

/** Rows that expired or were let go, newest first, each one key from restore. */
export function TodoExpiredView() {
  const expired = useExpiredQuery();
  const hidden = useTodoHidden((s) => s.hidden);
  const rows = (expired.data ?? []).filter((todo) => !hidden.has(todo.id));
  const { index, current, move, setCursorId } = useCursor(rows);
  const listRef = useScrollIntoView(index);

  useShortcutScope("expired");
  useScopeController("expired", {
    down: () => move(1),
    up: () => move(-1),
    restore: () => current && void restoreTodos([current]),
    back: () => getRuntimeNavigate().navigate("/todo"),
  });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-testid="todo-expired">
      <header className="mx-5 mt-4 flex max-w-[64rem] flex-wrap items-baseline gap-x-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-[15px] font-semibold">Expired</h2>
          <p className="mt-0.5 text-[12.5px] text-muted-foreground">
            Past their window, or let go in the catch-up. Restore puts one back in To do, and it
            stays until you act.
          </p>
        </div>
        <BackLink />
      </header>
      {expired.data && rows.length === 0 ? (
        <p className="mx-5 mt-6 text-[13px] text-muted-foreground">Nothing has expired.</p>
      ) : null}
      <ul ref={listRef} aria-label="Expired to-dos" className="max-w-[64rem] py-2">
        {rows.map((todo, position) => (
          <PlainRow
            key={todo.id}
            todo={todo}
            index={position}
            focused={position === index}
            onSelect={() => setCursorId(todo.id)}
          >
            <Button
              size="xs"
              variant="outline"
              onClick={(event) => {
                event.stopPropagation();
                void restoreTodos([todo]);
              }}
            >
              Restore {position === index ? <KeyChip className="ml-1 h-4 px-1">↵</KeyChip> : null}
            </Button>
          </PlainRow>
        ))}
      </ul>
    </div>
  );
}
