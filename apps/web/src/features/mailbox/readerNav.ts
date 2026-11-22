import { createContext, useContext } from "react";

import type { MailVerbHooks } from "@/features/mail-actions/mailVerbs";
import type { ExpectRemoval } from "@/hooks/useAdvanceOnRemoval";

export interface ReaderNav {
  threadIds: () => string[];
  open: (threadId: string, options?: { focusReader?: boolean }) => void;
  close: () => void;
  expectRemoval?: ExpectRemoval;
  isListed?: (threadId: string) => boolean;
  queueLabel?: string;
  intercept?: MailVerbHooks["intercept"];
}

export const ReaderNavContext = createContext<ReaderNav | null>(null);

export function useReaderNav(): ReaderNav | null {
  return useContext(ReaderNavContext);
}
