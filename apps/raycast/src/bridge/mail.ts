import { request } from "./client";
import type { Reply, Schemas } from "./types";

// The mailbox, search and thread routes answer with the bridge's own row
// JSON, which the OpenAPI spec does not describe yet; these are the fields
// the extension reads, as the web app's `features/mailbox/types.ts` has them.

export type Account = Schemas["AccountSummaryData"];
export type OneTimeCode = Schemas["OneTimeCodeData"];

export interface Address {
  name?: string | null;
  email: string;
}

/** One row of a mailbox or search list, a thread or a message. */
export interface MailRow {
  id: string;
  account_id?: string;
  thread_id: string;
  sender: string;
  sender_detail?: string | null;
  subject: string;
  snippet: string;
  date: string;
  unread: boolean;
  starred: boolean;
  has_attachments: boolean;
  message_count?: number | null;
}

/** A page of rows, with where the next page starts. */
export interface MailPage {
  rows: MailRow[];
  total?: number;
  nextOffset?: number;
}

interface Groups {
  groups: { rows: MailRow[] }[];
  has_more?: boolean;
  next_offset?: number | null;
}

export interface MessageBody {
  message_id: string;
  text_plain?: string | null;
  text_html?: string | null;
  attachments?: { filename: string; size_bytes: number }[];
}

export interface Thread {
  thread: { id: string; account_id: string; subject: string; message_count: number };
  /** Oldest first. */
  messages: (MailRow & { to?: Address[]; cc?: Address[] })[];
  bodies: MessageBody[];
}

/** Rows per page; the bridge groups them by day. */
export const PAGE_SIZE = 50;

/** Accounts the daemon syncs, for the account filter. */
export async function getAccounts(): Promise<Account[]> {
  const { accounts } = await request<Reply<"Accounts">>("/api/v1/platform/accounts");
  return accounts.filter((account) => account.enabled);
}

function page(result: Groups, total?: number): MailPage {
  return {
    rows: result.groups.flatMap((group) => group.rows),
    total,
    nextOffset: result.has_more ? (result.next_offset ?? undefined) : undefined,
  };
}

/** The inbox, newest first, one conversation per row. */
export async function getInbox(account: string | undefined, offset: number): Promise<MailPage> {
  const query = new URLSearchParams({
    lens_kind: "inbox",
    view: "threads",
    limit: String(PAGE_SIZE),
    offset: String(offset),
  });
  if (account) query.set("account", account);
  const { mailbox } = await request<{ mailbox: Groups & { counts?: { total?: number } } }>(
    `/api/v1/mail/mailbox?${query}`,
  );
  return page(mailbox, mailbox.counts?.total);
}

/**
 * Search all mail with mxr's query language (`from:`, `is:unread`,
 * `has:attachment`, dates, phrases), newest first, as `mxr search` does.
 */
export async function searchMail(
  q: string,
  account: string | undefined,
  offset: number,
): Promise<MailPage> {
  const query = new URLSearchParams({
    q,
    mode: "lexical",
    sort: "newest",
    scope: "threads",
    limit: String(PAGE_SIZE),
    offset: String(offset),
  });
  if (account) query.set("account", account);
  const result = await request<Groups & { total: number }>(`/api/v1/mail/search?${query}`);
  return page(result, result.total);
}

/** A conversation with its bodies. Fetching it marks nothing read. */
export function getThread(threadId: string): Promise<Thread> {
  return request<Thread>(`/api/v1/mail/threads/${encodeURIComponent(threadId)}`);
}

/** Mark messages read or unread, as the web reader does once a thread is open. */
export async function setRead(messageIds: string[], read: boolean): Promise<void> {
  await request("/api/v1/mail/mutations/read", { body: { message_ids: messageIds, read } });
}

/**
 * Recent one-time codes, current ones first, then expired, each newest
 * first (`GET /api/v1/mail/codes`, mxr 0.6.138 and later).
 */
export async function getCodes(account?: string, limit = 5): Promise<OneTimeCode[]> {
  const query = new URLSearchParams({ limit: String(limit) });
  if (account) query.set("account", account);
  return (await request<Reply<"OneTimeCodes">>(`/api/v1/mail/codes?${query}`)).codes;
}

/** "Google (accounts.google.com)": a borrowed brand shows beside its host. */
export function codeSender(code: OneTimeCode): string {
  return code.sender_host && code.sender_host.toLowerCase() !== code.sender_name.toLowerCase()
    ? `${code.sender_name} (${code.sender_host})`
    : code.sender_name;
}
