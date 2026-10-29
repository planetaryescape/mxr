/*
 * How the reader moves within the list it was opened from. The view that
 * owns the list (a mailbox lens or search results) provides this; the
 * reader never guesses its origin from the URL.
 */

import { createContext, useContext } from "react";

import type { MailVerbHooks } from "@/features/mail-actions/mailVerbs";
import type { ThreadRef } from "./rowKey";

export interface ReaderNav {
  /**
   * Conversations in list order, after pending actions are applied. Each
   * names its account: two accounts can hold the same thread id.
   */
  threads: () => ThreadRef[];
  open: (threadId: string, options?: { focusReader?: boolean; accountId?: string | null }) => void;
  close: () => void;
  /** Advance in this direction only if this reader's row actually leaves. */
  expectRemoval?: (
    threadId: string,
    direction: 1 | -1,
    accountId?: string | null,
  ) => (keepForRemoval?: boolean) => void;
  /** Read current list membership after the Archive refetch settles. */
  isListed?: (threadId: string, accountId?: string | null) => boolean;
  /** Queue label of the originating lens, for "route out of queue". */
  queueLabel?: string;
  /** The list's own handling of a verb (the desk's Done), for the reader too. */
  intercept?: MailVerbHooks["intercept"];
}

export const ReaderNavContext = createContext<ReaderNav | null>(null);

export function useReaderNav(): ReaderNav | null {
  return useContext(ReaderNavContext);
}
