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

export async function getNow(): Promise<NowData> {
  return (await request<Reply<"Now">>("/api/v1/mail/now")).now;
}

export async function getMessages(): Promise<MessagesData> {
  return (await request<Reply<"Messages">>("/api/v1/mail/people?limit=200")).messages;
}

export async function getTodo(): Promise<TodoRunway> {
  return (await request<Reply<"TodoRunway">>("/api/v1/mail/todos")).runway;
}

export async function getUpdates(): Promise<UpdatesDigest> {
  return (await request<Reply<"UpdatesDigest">>("/api/v1/mail/updates")).digest;
}

export async function getReading(): Promise<ReadingEdition> {
  return (await request<Reply<"ReadingEdition">>("/api/v1/mail/reading")).edition;
}

export async function getRecords(): Promise<RecordLedger> {
  return (await request<Reply<"RecordLedger">>("/api/v1/mail/records?limit=100")).ledger;
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
