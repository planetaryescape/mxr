import { openMailDialog } from "@/features/mail-actions/mailDialogStore";

import { toast } from "sonner";
import { create } from "zustand";

import { claimUndo, offerUndo } from "@/features/mail-actions/mailUndo";
import { markModeDone } from "@/features/modes/modeDone";
import { soundFor, VERB_FEEDBACK, type Verb } from "@/features/mail-actions/verbFeedback";
import { playSound } from "@/features/sound/player";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { plural } from "@/lib/format";
import { foldsFor, type LeaveVia } from "@/lib/leaving";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  createTodo,
  correctTodo,
  undoTodoCorrection,
  scheduleTodo,
  resolveTodoSuggestion,
  setCatchup,
  setTodoState,
  TODO_KEY,
  type Todo,
  type TodoChange,
} from "./api";

interface TodoHiddenState {
  leaving: ReadonlySet<string>;
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

function startLeaving(ids: readonly string[], how: LeaveVia = "pointer"): void {
  if (foldsFor(how)) useTodoHidden.getState().leave(ids);
  else useTodoHidden.getState().hide(ids);
}

async function refreshTodos({ guide = false }: { guide?: boolean } = {}): Promise<void> {
  const qc = getActiveQueryClient();
  if (!qc) return;
  await Promise.all([
    qc.invalidateQueries({ queryKey: TODO_KEY }),
    qc.invalidateQueries({ queryKey: ["now"] }),
    qc.invalidateQueries({ queryKey: ["rail"] }),
    guide ? qc.invalidateQueries({ queryKey: ["mode-guide"] }) : null,
  ]).catch(() => undefined);
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

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
        verb,
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

const reopen = (change: TodoChange) => undoBy(() => setTodoState(idsOf(change), "undo"));

function titled(change: TodoChange, verb: string): string {
  const [first] = change.changed;
  return change.changed.length === 1 && first
    ? `${verb}: ${first.title}`
    : `${verb} ${plural(change.changed.length, "to-do")}`;
}

export function markDone(todos: readonly Todo[], how: LeaveVia = "pointer") {
  const ids = todos.map((todo) => todo.id);
  if (ids.length === 0 || refuseWhileDaemonDown("tick it off")) return;
  startLeaving(ids, how);
  return withUndo("todo-done", () => setTodoState(ids, "done"), {
    message: (change) => titled(change, VERB_FEEDBACK["todo-done"].pastTense),
    reverse: reopen,
    failure: "Couldn't tick it off",
    ids,
  });
}

const pendingTicks = new Set<string>();

export async function tickOff(todo: Todo, how: LeaveVia = "pointer"): Promise<void> {
  if (pendingTicks.has(todo.id)) return;
  pendingTicks.add(todo.id);
  try {
    await tickOffOnce(todo, how);
  } finally {
    pendingTicks.delete(todo.id);
  }
}

async function tickOffOnce(todo: Todo, how: LeaveVia): Promise<void> {
  const threadId = todo.thread_id;
  if (!threadId) {
    await markDone([todo], how);
    return;
  }
  startLeaving([todo.id], how);
  try {
    await markModeDone("todo", [threadId], { todoIds: [todo.id] });
  } finally {
    await refreshTodos({ guide: true });
    useTodoHidden.getState().show([todo.id]);
  }
}

export function resolveSuggestion(todo: Todo, action: "preview" | "accept" | "dismiss" | "undo") {
  const offer = todo.resolution;
  if (!offer || refuseWhileDaemonDown("review the email")) return;
  if (action === "preview") {
    return resolveTodoSuggestion(todo.id, offer.id, "preview", true).catch((error) => {
      toast.error("Couldn't review that email update", { description: errorText(error) });
      return null;
    });
  }
  if (action === "accept") {
    const verb: Verb = offer.action === "complete" ? "todo-done" : "todo-edit";
    return withUndo(verb, () => resolveTodoSuggestion(todo.id, offer.id, "accept"), {
      message: (change) => change.summary,
      reverse: () => undoBy(() => resolveTodoSuggestion(todo.id, offer.id, "undo")),
      failure: "Couldn't apply that email update",
      ids: [todo.id],
    });
  }
  const run = () => resolveTodoSuggestion(todo.id, offer.id, action);
  return run()
    .then((change) => {
      toast.success(change.summary);
      return change;
    })
    .catch((error) => {
      toast.error(
        action === "dismiss" ? "Couldn't dismiss that offer" : "Couldn't undo that offer",
        {
          description: errorText(error),
        },
      );
      return null;
    })
    .finally(() => refreshTodos());
}

export function markNotTodo(todos: readonly Todo[], _how: LeaveVia = "pointer") {
  const todo = todos[0];
  if (todo) openMailDialog({ kind: "todo-correct", todo });
}

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

export function scheduleRow(todo: Todo, at: string, label: string) {
  if (refuseWhileDaemonDown("schedule it")) return;
  const previous = todo.scheduled_for ?? null;
  return withUndo("todo-schedule", () => scheduleTodo(todo.id, at), {
    message: () => `Scheduled for ${label}: ${todo.title}`,
    reverse: () => undoBy(() => scheduleTodo(todo.id, previous)),
    failure: "Couldn't schedule it",
  });
}

export function editRow(todo: Todo, edits: { field: string; value: string }[]) {
  if (edits.length === 0 || refuseWhileDaemonDown("change it")) return;
  return correctRow(todo, "wrong_details", edits);
}

export function correctRow(
  todo: Todo,
  reason: "not_a_todo" | "not_mine" | "already_done" | "wrong_details",
  edits: { field: string; value: string }[] = [],
) {
  if (refuseWhileDaemonDown("correct it")) return;
  return withUndo(
    reason === "wrong_details" ? "todo-edit" : "todo-dismiss",
    () => correctTodo(todo.id, reason, edits),
    {
      message: (change) => change.summary,
      reverse: (change) => {
        const token = change.correction_undo;
        return token
          ? undoBy(() => undoTodoCorrection(todo.id, token))
          : () => Promise.resolve(false);
      },
      failure: "Couldn't save that correction",
    },
  );
}

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

export function makeTodo(input: { messageId: string; title: string; due?: string }) {
  if (refuseWhileDaemonDown("add a to-do")) return;
  return withUndo("todo-create", () => createTodo(input), {
    message: (change) => titled(change, VERB_FEEDBACK["todo-create"].pastTense),
    reverse: (change) => undoBy(() => setTodoState(idsOf(change), "dismiss")),
    failure: "Couldn't add the to-do",
  });
}

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

function undecide(account: string | null, change: TodoChange) {
  return undoBy(() => setCatchup(account, { decision: "undecide", todo_ids: idsOf(change) }));
}

export function previewLetGoAll(account: string | null) {
  return setCatchup(account, { decision: "let_go_all" }, true);
}

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
