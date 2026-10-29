/* Moving between conversations in the list the reader was opened from. */

import { useCallback, useMemo } from "react";

import type { ReaderNav } from "@/features/mailbox/readerNav";
import { conversationKey, type ThreadRef } from "@/features/mailbox/rowKey";

export function useThreadSiblings(
  threadId: string,
  nav: ReaderNav | null,
  /** The open conversation's account: a thread id can be held by two accounts. */
  accountId?: string | null,
) {
  // The list's conversations, one per (account, thread), in list order.
  const siblings = useCallback(() => {
    const seen = new Set<string>();
    return (nav?.threads() ?? []).filter((ref) => {
      const key = conversationKey(ref.thread_id, ref.account_id);
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [nav]);
  const indexOf = useCallback(
    (refs: ThreadRef[]) => {
      const key = conversationKey(threadId, accountId);
      const exact = refs.findIndex((ref) => conversationKey(ref.thread_id, ref.account_id) === key);
      // No account known, or none on the listed row (older data): match on the thread alone.
      if (exact >= 0) return exact;
      return refs.findIndex((ref) => ref.thread_id === threadId && (!accountId || !ref.account_id));
    },
    [accountId, threadId],
  );
  const position = useMemo(() => {
    const refs = siblings();
    const index = indexOf(refs);
    return index >= 0 ? { index, total: refs.length } : null;
  }, [indexOf, siblings]);

  const step = useCallback(
    (delta: 1 | -1) => {
      const refs = siblings();
      const index = indexOf(refs);
      const next = index >= 0 ? refs[index + delta] : undefined;
      if (next) nav?.open(next.thread_id, { focusReader: true, accountId: next.account_id });
      return Boolean(next);
    },
    [indexOf, nav, siblings],
  );

  // After the conversation leaves the list: open the next one, like the
  // TUI's archive-and-advance, or go back to the list at the end.
  const leave = useCallback(
    (direction: 1 | -1 = 1) => {
      const refs = siblings();
      const index = indexOf(refs);
      const next = refs[index + direction] ?? refs[index - direction];
      if (index >= 0 && next) {
        nav?.open(next.thread_id, { focusReader: true, accountId: next.account_id });
      } else nav?.close();
    },
    [indexOf, nav, siblings],
  );

  return { position, step, leave };
}
