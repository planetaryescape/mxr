/* Moving between conversations in the list the reader was opened from. */

import { useCallback, useMemo } from "react";

import type { ReaderNav } from "@/features/mailbox/readerNav";

export function useThreadSiblings(threadId: string, nav: ReaderNav | null) {
  const siblings = useCallback(() => nav?.threadIds() ?? [], [nav]);
  const position = useMemo(() => {
    const ids = siblings();
    const index = ids.indexOf(threadId);
    return index >= 0 ? { index, total: ids.length } : null;
  }, [threadId, siblings]);

  const step = useCallback(
    (delta: 1 | -1) => {
      const ids = siblings();
      const index = ids.indexOf(threadId);
      const next = index >= 0 ? ids[index + delta] : undefined;
      if (next) nav?.open(next, { focusReader: true });
      return Boolean(next);
    },
    [threadId, nav, siblings],
  );

  // After the conversation leaves the list: open the next one, like the
  // TUI's archive-and-advance, or go back to the list at the end.
  const leave = useCallback(
    (direction: 1 | -1 = 1) => {
      const ids = siblings();
      const index = ids.indexOf(threadId);
      const next = ids[index + direction] ?? ids[index - direction];
      if (index >= 0 && next && next !== threadId) nav?.open(next, { focusReader: true });
      else nav?.close();
    },
    [threadId, nav, siblings],
  );

  return { position, step, leave };
}
