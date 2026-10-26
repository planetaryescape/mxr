/*
 * To do over the bridge. The daemon builds the bands, labels and why lines
 * (`GetTodoRunway`), and every mutation answers with the rows as they are
 * after it (`TodoChange`); this file only moves them.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { browserTimeZone } from "@/features/time/api";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type Todo = Schemas["TodoData"];
export type TodoRunway = Schemas["TodoRunwayData"];
export type TodoCatchup = Schemas["TodoCatchupData"];
export type TodoChange = Schemas["TodoChangeData"];
export type TodoField = Schemas["TodoFieldData"];
type TodoStateAction = Schemas["TodoStateActionData"];
type TodoResolutionAction = Schemas["TodoResolutionActionData"];
type TodoCatchupDecision = Schemas["TodoCatchupDecisionData"];

type Runway = Extract<Schemas["ResponseData"], { kind: "TodoRunway" }>;
type Catchup = Extract<Schemas["ResponseData"], { kind: "TodoCatchup" }>;
type Todos = Extract<Schemas["ResponseData"], { kind: "Todos" }>;
type Change = Extract<Schemas["ResponseData"], { kind: "TodoChange" }>;

/** Every To do query starts with this, so one invalidation refreshes them. */
export const TODO_KEY = ["todos"] as const;

export type MicrosoftTodoLink = Schemas["MicrosoftTodoSendData"];

export function useMicrosoftTodoStatus() {
  return useQuery({
    queryKey: ["microsoft-todo", "status"],
    queryFn: async () => {
      const answer = await apiFetch<
        Extract<Schemas["ResponseData"], { kind: "MicrosoftTodoStatus" }>
      >("/api/v1/mail/todos/microsoft");
      return answer.status;
    },
    staleTime: 60_000,
    retry: false,
  });
}

export async function linkMicrosoftTodo(
  id: string,
  fingerprint: string | undefined,
  accountId: string,
): Promise<MicrosoftTodoLink> {
  const answer = await apiFetch<Extract<Schemas["ResponseData"], { kind: "MicrosoftTodoSend" }>>(
    `/api/v1/mail/todos/${encodeURIComponent(id)}/microsoft`,
    {
      method: "POST",
      body: { account_id: accountId, dry_run: fingerprint === undefined, fingerprint },
    },
  );
  return answer.result;
}

export async function syncMicrosoftTodo(id: string, dryRun: boolean, accountId: string) {
  const answer = await apiFetch<Extract<Schemas["ResponseData"], { kind: "MicrosoftTodoSync" }>>(
    "/api/v1/mail/todos/microsoft/sync",
    { method: "POST", body: { todo_id: id, account_id: accountId, dry_run: dryRun } },
  );
  return answer.sync;
}

function accountQuery(account: string | null, extra: Record<string, string> = {}): string {
  const query = new URLSearchParams(extra);
  if (account) query.set("account", account);
  const text = query.toString();
  return text ? `?${text}` : "";
}

export async function fetchRunway(account: string | null, markSeen: boolean): Promise<TodoRunway> {
  const answer = await apiFetch<Runway>(
    `/api/v1/mail/todos${accountQuery(account, markSeen ? { mark_seen: "true" } : {})}`,
  );
  return answer.runway;
}

async function fetchCatchup(account: string | null): Promise<TodoCatchup> {
  const answer = await apiFetch<Catchup>(`/api/v1/mail/todos/catchup${accountQuery(account)}`);
  return answer.catchup;
}

async function fetchExpired(account: string | null): Promise<Todo[]> {
  const answer = await apiFetch<Todos>(`/api/v1/mail/todos/in/expired${accountQuery(account)}`);
  return answer.todos;
}

export function useCatchupQuery(enabled = true) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...TODO_KEY, "catchup", account ?? "all"],
    queryFn: () => fetchCatchup(account),
    enabled,
  });
}

export function useExpiredQuery(enabled = true) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...TODO_KEY, "expired", account ?? "all"],
    queryFn: () => fetchExpired(account),
    enabled,
  });
}

async function change(path: string, body: unknown): Promise<TodoChange> {
  const answer = await apiFetch<Change>(path, { method: "POST", body });
  return answer.change;
}

export function setTodoState(ids: string[], action: TodoStateAction, dryRun = false) {
  return change("/api/v1/mail/todos/state", { todo_ids: ids, action, dry_run: dryRun });
}

export function resolveTodoSuggestion(
  todoId: string,
  suggestionId: string,
  action: TodoResolutionAction,
  dryRun = false,
) {
  return change(`/api/v1/mail/todos/${encodeURIComponent(todoId)}/resolution`, {
    suggestion_id: suggestionId,
    action,
    dry_run: dryRun,
  });
}

/** `when` is a phrase or the RFC 3339 instant the time field resolved; null clears it. */
export function scheduleTodo(id: string, when: string | null) {
  return change(`/api/v1/mail/todos/${encodeURIComponent(id)}/schedule`, {
    when,
    time_zone: browserTimeZone(),
  });
}

export function editTodo(id: string, edits: { field: string; value: string }[]) {
  return change(`/api/v1/mail/todos/${encodeURIComponent(id)}/edit`, {
    edits,
    time_zone: browserTimeZone(),
  });
}

export function createTodo(input: { messageId: string; title: string; due?: string }) {
  return change("/api/v1/mail/todos", {
    message_id: input.messageId,
    title: input.title,
    due: input.due || undefined,
    time_zone: browserTimeZone(),
  });
}

export function setCatchup(account: string | null, decision: TodoCatchupDecision, dryRun = false) {
  return change("/api/v1/mail/todos/catchup", {
    account_id: account ?? undefined,
    decision,
    dry_run: dryRun,
  });
}

export function correctTodo(todoId: string, reason: "not_a_todo" | "not_mine" | "already_done" | "wrong_details", edits: { field: string; value: string }[] = [], dryRun = false) {
  return change(`/api/v1/mail/todos/${encodeURIComponent(todoId)}/correct`, { reason, edits, time_zone: browserTimeZone(), dry_run: dryRun });
}
export function undoTodoCorrection(todoId: string, correctionId: string) {
  return change(`/api/v1/mail/todos/${encodeURIComponent(todoId)}/correction/undo`, { correction_id: correctionId, dry_run: false });
}
