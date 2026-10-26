import { request } from "./client";
import type {
  MessagesData,
  ModeKind,
  NowData,
  ReadingEdition,
  RecordLedger,
  Reply,
  TodoRunway,
  UpdatesDigest,
} from "./types";

// Reads never mark anything seen or visited, so they stay answerable from
// the daemon's ready-made views and leave the web app's "since you were
// last here" lines alone.

/** A mode read, scoped to one account when `account` is set. */
function read<T>(path: string, account: string | undefined): Promise<T> {
  if (!account) return request<T>(path);
  const separator = path.includes("?") ? "&" : "?";
  return request<T>(`${path}${separator}account=${encodeURIComponent(account)}`);
}

export async function getNow(account?: string): Promise<NowData> {
  return (await read<Reply<"Now">>("/api/v1/mail/now", account)).now;
}

/** What Messages keeps: a turn and the two toggles, as the daemon names them. */
export interface MessagesFilter {
  turn?: "all" | "mine" | "theirs";
  unread?: boolean;
  pace?: boolean;
}

export async function getMessages(
  account?: string,
  filter: MessagesFilter = {},
): Promise<MessagesData> {
  const query = new URLSearchParams({ limit: "200" });
  if (filter.turn) query.set("turn", filter.turn);
  if (filter.unread) query.set("unread", "true");
  if (filter.pace) query.set("pace", "true");
  return (await read<Reply<"Messages">>(`/api/v1/mail/people?${query}`, account)).messages;
}

export async function getTodo(account?: string): Promise<TodoRunway> {
  return (await read<Reply<"TodoRunway">>("/api/v1/mail/todos", account)).runway;
}

export async function getUpdates(account?: string): Promise<UpdatesDigest> {
  return (await read<Reply<"UpdatesDigest">>("/api/v1/mail/updates", account)).digest;
}

export async function getReading(account?: string): Promise<ReadingEdition> {
  return (await read<Reply<"ReadingEdition">>("/api/v1/mail/reading", account)).edition;
}

export async function getRecords(account?: string): Promise<RecordLedger> {
  return (await read<Reply<"RecordLedger">>("/api/v1/mail/records?limit=100", account)).ledger;
}

/** The modes a conversation can be marked done in. */
export type DoneMode = Exclude<ModeKind, "archive">;

/** "Done here": the conversations leave `mode` until someone writes again. */
export function markDone(
  mode: DoneMode,
  threadIds: string[],
  dryRun: boolean,
  todoIds?: string[],
): Promise<Reply<"ModeDone">> {
  return request(`/api/v1/mail/modes/${mode}/done`, {
    body: { thread_ids: threadIds, todo_ids: todoIds, dry_run: dryRun },
  });
}

export function tickOffTodo(todoId: string, dryRun: boolean): Promise<Reply<"TodoChange">> {
  return request("/api/v1/mail/todos/state", {
    body: { todo_ids: [todoId], action: "done", dry_run: dryRun },
  });
}

/** Let go of the digest the user is looking at; `selection_token` refuses a changed one. */
export function letGoDigest(
  digest: UpdatesDigest,
  dryRun: boolean,
): Promise<Reply<"UpdatesLetGo">> {
  return request("/api/v1/mail/updates/let-go", {
    body: { cut: digest.cut.at, selection_token: digest.selection_token, dry_run: dryRun },
  });
}

export function saveForLater(itemKey: string, dryRun: boolean): Promise<Reply<"ReadingLater">> {
  return request("/api/v1/mail/reading/later", {
    body: { item_keys: [itemKey], later: true, dry_run: dryRun },
  });
}

export function undoMutation(mutationId: string): Promise<unknown> {
  return request("/api/v1/mail/mutations/undo", { body: { mutation_id: mutationId } });
}
