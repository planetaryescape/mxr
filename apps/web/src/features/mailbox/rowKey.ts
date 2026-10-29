import type { MessageRowView } from "./types";

/**
 * A row's identity in the list. A conversation row is its thread: the
 * message that represents it can change under the cursor (search picks the
 * best-matching message, and a read or archive elsewhere in the thread can
 * change which one that is), and the cursor, selection and DOM id must not
 * move when it does. Attachment search rows share their message's id, one
 * per attachment, so they need the attachment id too.
 */
export function rowKey(row: MessageRowView): string {
  // A legacy thread id can be held by two accounts, so the row's account is
  // part of its identity: selecting one account's row must not select the other's.
  if (row.kind === "thread") return threadRowKey(row.thread_id, row.account_id);
  return row.kind === "attachment" && row.attachment_id ? `${row.id}:${row.attachment_id}` : row.id;
}

/** Identity of a conversation row: the thread within its account. */
export function threadRowKey(threadId: string, accountId?: string | null): string {
  return accountId ? `thread-${accountId}:${threadId}` : `thread-${threadId}`;
}

/** A conversation as a row names it. Without an account (older data) it is the bare thread id. */
export interface ThreadRef {
  thread_id: string;
  account_id?: string | null;
}

/**
 * What names a conversation in a set or map that outlives one list: its
 * thread within its account, since two accounts can hold one thread id.
 * Data with no account is named by the bare thread id.
 */
export function conversationKey(threadId: string, accountId?: string | null): string {
  return accountId ? threadRowKey(threadId, accountId) : threadId;
}

/**
 * Whether a set of conversation keys holds this conversation. A bare entry
 * (a caller that had no account) covers the thread in every account; a
 * qualified one covers only its own account's row.
 */
export function holdsThread(
  keys: ReadonlySet<string>,
  threadId: string,
  accountId?: string | null,
): boolean {
  return keys.has(conversationKey(threadId, accountId)) || keys.has(threadId);
}

/**
 * The key of the list entry the URL names: the thread with the account the
 * URL carries, else the first listed copy of that thread (an older link).
 */
export function selectedKey(
  refs: Iterable<ThreadRef>,
  threadId: string,
  accountId?: string | null,
): string {
  if (accountId) return conversationKey(threadId, accountId);
  for (const ref of refs) {
    if (ref.thread_id === threadId) return conversationKey(ref.thread_id, ref.account_id);
  }
  return threadId;
}

/** The `thread_accounts` pairs for refs that know their account. */
export function threadAccounts(
  refs: readonly ThreadRef[],
): { thread_id: string; account_id: string }[] {
  const pairs = new Map<string, { thread_id: string; account_id: string }>();
  for (const ref of refs) {
    if (ref.account_id) {
      pairs.set(conversationKey(ref.thread_id, ref.account_id), {
        thread_id: ref.thread_id,
        account_id: ref.account_id,
      });
    }
  }
  return [...pairs.values()];
}
