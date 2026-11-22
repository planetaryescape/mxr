import { toast } from "sonner";

import { deferThreadsAt, deskLaneOf } from "@/features/desk/deskLater";
import { setReplyLater } from "@/features/reply-queue/api";

import { performMailAction } from "./mailMutations";
import { invalidateMailQueries } from "./mailQueryInvalidation";
import type { MailTarget } from "./target";
import { toneFor, VERB_FEEDBACK } from "./verbFeedback";

export interface NotNowPicker {
  title: string;
  description: string;
  later?: { label: string; hint: string; run: () => Promise<unknown> | void };
  timeVerb: string;
  at: (until: string, label: string) => Promise<unknown> | void;
}

export async function replyLaterNow(messageId: string): Promise<void> {
  try {
    await setReplyLater(messageId, true);
  } catch (error) {
    toast.error("Reply later failed", {
      description: error instanceof Error ? error.message : String(error),
    });
    return;
  }
  void invalidateMailQueries();
  toast[toneFor("reply-later")](VERB_FEEDBACK["reply-later"].pastTense, {
    action: {
      label: "Undo",
      onClick: () => void setReplyLater(messageId, false).then(() => invalidateMailQueries()),
    },
  });
}

const REPLY_LATER = {
  label: "Later",
  hint: "No time: it joins your reply queue now",
} as const;

function snooze(messageIds: string[], until: string, label: string) {
  return performMailAction("snooze", messageIds, { until, payload: { untilLabel: label } });
}

export function mailNotNow(
  target: MailTarget,
  subject: string,
  hooks: { onSnoozed?: () => void; onLater?: () => void; onDesk?: boolean } = {},
): NotNowPicker {
  const messageId = target.conversations === 1 ? target.primary?.id : undefined;
  const waiting = target.threadId ? deskLaneOf(target.threadId) === "waiting" : false;
  if (hooks.onDesk && target.threadId && target.conversations === 1) {
    return conversationNotNow({ threadId: target.threadId, messageId, subject, waiting });
  }
  return messageNotNow({
    messageIds: target.messageIds,
    replyLaterId: waiting ? undefined : messageId,
    subject,
    ...hooks,
  });
}

export function messageNotNow({
  messageIds,
  replyLaterId,
  subject,
  onSnoozed,
  onLater,
}: {
  messageIds: string[];
  replyLaterId?: string;
  subject: string;
  onSnoozed?: () => void;
  onLater?: () => void;
}): NotNowPicker {
  return {
    title: "Not now",
    description: `${subject} leaves the inbox and comes back then.`,
    later: replyLaterId
      ? {
          ...REPLY_LATER,
          run: () => {
            onLater?.();
            return replyLaterNow(replyLaterId);
          },
        }
      : undefined,
    timeVerb: "Snooze",
    at: (until, label) => {
      onSnoozed?.();
      return snooze(messageIds, until, label);
    },
  };
}

export function conversationNotNow({
  threadId,
  messageId,
  subject,
  waiting,
}: {
  threadId: string;
  messageId?: string;
  subject: string;
  waiting: boolean;
}): NotNowPicker {
  return {
    title: waiting ? "Bring back if nobody replies" : "Not now",
    description: waiting
      ? `${subject} leaves Waiting on and comes back then, unless someone replies.`
      : `${subject} leaves the desk and comes back to You owe and your reply queue then.`,
    later:
      !waiting && messageId ? { ...REPLY_LATER, run: () => replyLaterNow(messageId) } : undefined,
    timeVerb: "Set",
    at: (until, label) => deferThreadsAt([threadId], until, label),
  };
}

export const READING_LATER = {
  label: "Later",
  hint: "No time: it stays on your Later shelf",
} as const;
export const READING_KEEP = {
  label: "Keep it on Later",
  hint: "No time: it stays on the shelf",
} as const;

export function readingNotNow({
  messageId,
  title,
  later,
}: {
  messageId: string;
  title: string;
  later: { label: string; hint: string; run: () => Promise<unknown> | void };
}): NotNowPicker {
  return {
    title: "Not now",
    description: `${title}: keep it for later, or let the email come back at a time.`,
    later,
    timeVerb: "Snooze",
    at: (until, label) => snooze([messageId], until, label),
  };
}
