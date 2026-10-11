/*
 * How the reader moves within the list it was opened from. The view that
 * owns the list (a mailbox lens or search results) provides this; the
 * reader never guesses its origin from the URL.
 */

import { createContext, useContext } from "react";

import type { MailVerbHooks } from "@/features/mail-actions/mailVerbs";
import type { ExpectRemoval } from "@/hooks/useAdvanceOnRemoval";

export interface ReaderNav {
  /** Thread ids in list order, after pending actions are applied. */
  threadIds: () => string[];
  open: (threadId: string, options?: { focusReader?: boolean }) => void;
  close: () => void;
  /** Advance in this direction only if this reader's row actually leaves. */
  expectRemoval?: ExpectRemoval;
  /** Read current list membership after the Archive refetch settles. */
  isListed?: (threadId: string) => boolean;
  /** Queue label of the originating lens, for "route out of queue". */
  queueLabel?: string;
  /** The list's own handling of a verb (the desk's Done), for the reader too. */
  intercept?: MailVerbHooks["intercept"];
}

export const ReaderNavContext = createContext<ReaderNav | null>(null);

export function useReaderNav(): ReaderNav | null {
  return useContext(ReaderNavContext);
}
