/*
 * To do's verbs: tick off, not a to-do, schedule, edit, make one, and the
 * catch-up's keep and let go. Each is one daemon request; the row leaves
 * at once, and `u` or the toast's Undo reverses it with another request
 * (reopen, or the previous date), since to-dos are not mail mutations.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { claimUndo, offerUndo } from "@/features/mail-actions/mailUndo";
import { soundFor, VERB_FEEDBACK, type Verb } from "@/features/mail-actions/verbFeedback";
import { playSound } from "@/features/sound/player";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { plural } from "@/lib/format";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  createTodo,
  editTodo,
  scheduleTodo,
  setCatchup,
  setTodoState,
  TODO_KEY,
  type Todo,
  type TodoChange,
} from "./api";

interface TodoHiddenState {
  /** Rows folding up after done or not a to-do: still drawn, animating out. */
  leaving: ReadonlySet<string>;
  /** Rows gone from the view until the daemon's answer is refetched. */
  hidden: ReadonlySet<string>;
  leave: (ids: readonly string[]) => void;
  hide: (ids: readonly string[]) => void;
  show: (ids: readonly string[]) => void;
}

const without = (set: ReadonlySet<string>, ids: readonly string[]) =>
  new Set([...set].filter((id) => !ids.includes(id)));

export const useTodoHidden = create<TodoHiddenState>((set) => ({
  leaving: new Set(),
  hidden: new Set(),
  leave: (ids) => set((s) => ({ leaving: new Set([...s.leaving, ...ids]) })),
  hide: (ids) =>
    set((s) => ({ leaving: without(s.leaving, ids), hidden: new Set([...s.hidden, ...ids]) })),
  show: (ids) =>
    set((s) =>
      ids.some((id) => s.hidden.has(id) || s.leaving.has(id))
        ? { leaving: without(s.leaving, ids), hidden: without(s.hidden, ids) }
        : s,
    ),
}));

/** Reduced motion: rows disappear at once instead of folding up. */
function motionReduced(): boolean {
  return document.documentElement.dataset.motion === "reduced";
}

/** Start a row's exit: fold it up, or drop it at once under reduced motion. */
function startLeaving(ids: readonly string[]): void {
  if (motionReduced()) useTodoHidden.getState().hide(ids);
  else useTodoHidden.getState().leave(ids);
}

/**
 * Refetch every To do view. Ticking off also retires the mode's card in
 * the daemon, so done refetches the guide too.
 */
async function refreshTodos({ guide = false }: { guide?: boolean } = {}): Promise<void> {
  const qc = getActiveQueryClient();
  if (!qc) return;
  await Promise.all([
    qc.invalidateQueries({ queryKey: TODO_KEY }),
    guide ? qc.invalidateQueries({ queryKey: ["mode-guide"] }) : null,
  ]).catch(() => undefined);
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Run a change whose undo is another request: hold the undo slot first so
 * an early `u` waits for the answer, then offer the toast's Undo.
 */
async function withUndo(
  verb: Verb,
  run: () => Promise<TodoChange>,
  options: {
    message: (change: TodoChange) => string;
    reverse: ((change: TodoChange) => () => Promise<boolean>) | null;
    failure: string;
    ids?: readonly string[];
  },
): Promise<TodoChange | null> {
  const claim = claimUndo();
  try {
    const change = await run();
    const did = change.changed.length > 0;
    const reverse = did && options.reverse ? options.reverse(change) : null;
    const sound = did ? soundFor(verb) : null;
    if (sound) playSound(sound);
    claim.settle(
      offerUndo(
        did ? options.message(change) : change.summary,
        `${verb}-${change.changed.map((todo) => todo.id).join(",")}`,
        reverse,
        claim.run,
      ),
    );
    return change;
  } catch (error) {
    claim.settle(null);
    toast.error(options.failure, { description: errorText(error) });
    return null;
  } finally {
    await refreshTodos({ guide: verb === "todo-done" });
    if (options.ids) useTodoHidden.getState().show(options.ids);
  }
}

/** An undo that is one more request: the guard, both toasts and the refresh. */
function undoBy(request: () => Promise<unknown>): () => Promise<boolean> {
  return async () => {
    if (refuseWhileDaemonDown("undo")) return false;
    try {
      await request();
      toast.success("Undone");
      return true;
    } catch (error) {
      toast.error("Undo failed", { description: errorText(error) });
      return false;
    } finally {
      await refreshTodos();
    }
  };
}

const idsOf = (change: TodoChange) => change.changed.map((todo) => todo.id);

/** Reopen rows, as the undo of done or not a to-do. */
const reopen = (change: TodoChange) => undoBy(() => setTodoState(idsOf(change), "undo"));

function titled(change: TodoChange, verb: string): string {
  const [first] = change.changed;
  return change.changed.length === 1 && first
    ? `${verb}: ${first.title}`
    : `${verb} ${plural(change.changed.length, "to-do")}`;
}

/** `e`: tick off. The row folds up and joins Done this week. */
export function markDone(todos: readonly Todo[]) {
  const ids = todos.map((todo) => todo.id);
  if (ids.length === 0 || refuseWhileDaemonDown("tick it off")) return;
  startLeaving(ids);
  return withUndo("todo-done", () => setTodoState(ids, "done"), {
    message: (change) => titled(change, VERB_FEEDBACK["todo-done"].pastTense),
    reverse: reopen,
    failure: "Couldn't tick it off",
    ids,
  });
}

/** `X`: not a to-do, kept as a correction so that email never comes back. */
export function markNotTodo(todos: readonly Todo[]) {
  const ids = todos.map((todo) => todo.id);
  if (ids.length === 0 || refuseWhileDaemonDown("change it")) return;
  startLeaving(ids);
  return withUndo("todo-dismiss", () => setTodoState(ids, "dismiss"), {
    message: (change) => titled(change, VERB_FEEDBACK["todo-dismiss"].pastTense),
    reverse: reopen,
    failure: "Couldn't change it",
    ids,
  });
}

/** Undo from a list: bring rows back (Done this week, the Expired list). */
export async function restoreTodos(todos: readonly Todo[]): Promise<void> {
  if (todos.length === 0 || refuseWhileDaemonDown("restore it")) return;
  try {
    const change = await setTodoState(
      todos.map((todo) => todo.id),
      "undo",
    );
    toast.success(change.changed.length > 0 ? titled(change, "Back in To do") : change.summary);
  } catch (error) {
    toast.error("Couldn't restore it", { description: errorText(error) });
  } finally {
    await refreshTodos();
  }
}

/**
 * `Z`: show the row on the chosen day. Undo puts back the date it had
 * (its own earlier date, or none).
 */
export function scheduleRow(todo: Todo, at: string, label: string) {
  if (refuseWhileDaemonDown("schedule it")) return;
  const previous = todo.scheduled_for ?? null;
  return withUndo("todo-schedule", () => scheduleTodo(todo.id, at), {
    message: () => `Scheduled for ${label}: ${todo.title}`,
    reverse: () => undoBy(() => scheduleTodo(todo.id, previous)),
    failure: "Couldn't schedule it",
  });
}

/** `,`: correct fields. Undo writes the earlier values back. */
export function editRow(todo: Todo, edits: { field: string; value: string }[]) {
  if (edits.length === 0 || refuseWhileDaemonDown("change it")) return;
  const before = edits.map(({ field }) => ({ field, value: currentValue(todo, field) }));
  return withUndo("todo-edit", () => editTodo(todo.id, edits), {
    message: (change) => titled(change, VERB_FEEDBACK["todo-edit"].pastTense),
    reverse: () => undoBy(() => editTodo(todo.id, before)),
    failure: "Couldn't change it",
  });
}

/** What an edit field holds now, in the form `UpdateTodo` reads back. */
export function currentValue(todo: Todo, field: string): string {
  switch (field) {
    case "title":
      return todo.title;
    case "counterparty":
      return todo.counterparty ?? "";
    case "amount":
      return todo.amount?.display ?? "";
    case "due":
      return todo.due_at ?? "";
    case "kind":
      return todo.kind;
    default:
      return "";
  }
}

/** `t` on a conversation: make a to-do from its newest message. */
export function makeTodo(input: { messageId: string; title: string; due?: string }) {
  if (refuseWhileDaemonDown("add a to-do")) return;
  return withUndo("todo-create", () => createTodo(input), {
    message: (change) => titled(change, VERB_FEEDBACK["todo-create"].pastTense),
    reverse: (change) => undoBy(() => setTodoState(idsOf(change), "dismiss")),
    failure: "Couldn't add the to-do",
  });
}

/** Catch-up: keep rows (they join the runway) or let them go (Expired). */
export function decideCatchup(
  account: string | null,
  todos: readonly Todo[],
  decision: "keep" | "let_go",
) {
  const ids = todos.map((todo) => todo.id);
  if (ids.length === 0 || refuseWhileDaemonDown("change the catch-up")) return;
  startLeaving(ids);
  const verb: Verb = decision === "keep" ? "todo-keep" : "todo-let-go";
  return withUndo(verb, () => setCatchup(account, { decision, todo_ids: ids }), {
    message: (change) => titled(change, VERB_FEEDBACK[verb].pastTense),
    reverse: (change) => undecide(account, change),
    failure: "Couldn't change the catch-up",
    ids,
  });
}

/** Undo of keep or let go: the rows wait in the catch-up again, undecided. */
function undecide(account: string | null, change: TodoChange) {
  return undoBy(() => setCatchup(account, { decision: "undecide", todo_ids: idsOf(change) }));
}

/** The dry run behind "Let go of all": exactly the rows the commit takes. */
export function previewLetGoAll(account: string | null) {
  return setCatchup(account, { decision: "let_go_all" }, true);
}

/**
 * Commit "Let go of all" for exactly the rows its preview listed, so what
 * was shown is what changes even if new rows joined the batch meanwhile.
 */
export function letGoAll(account: string | null, previewed: readonly Todo[]) {
  const ids = previewed.map((todo) => todo.id);
  if (ids.length === 0 || refuseWhileDaemonDown("let go of them")) return;
  startLeaving(ids);
  return withUndo("todo-let-go", () => setCatchup(account, { decision: "let_go", todo_ids: ids }), {
    message: (change) => `Let go of ${plural(change.changed.length, "to-do")}`,
    reverse: (change) => undecide(account, change),
    failure: "Couldn't let go of them",
    ids,
  });
}
