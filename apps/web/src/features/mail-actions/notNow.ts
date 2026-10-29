/*
 * Not now (`b`, D173): one picker everywhere, "Later (no time)" first,
 * then times. Each place says what Later and a time do there, with the
 * daemon request it always sent: the untimed reply later and snooze for
 * mail and Now, reply later and a desk time for Messages, the Later shelf
 * and snooze for Reading, a scheduled day for To do.
 */

import { toast } from "sonner";

import { deferThreadsAt, deskLaneOf } from "@/features/desk/deskLater";
import { setReplyLater } from "@/features/reply-queue/api";

import { performMailAction } from "./mailMutations";
import { invalidateMailQueries } from "./mailQueryInvalidation";
import type { MailTarget } from "./target";
import { toneFor, VERB_FEEDBACK } from "./verbFeedback";

/** What the Not-now picker offers and does, built by the place that opens it. */
export interface NotNowPicker {
  /** The dialog's title: "Not now". */
  title: string;
  /** What leaves and how it comes back. */
  description: string;
  /** The first option, with no time. Absent where nothing untimed applies. */
  later?: { label: string; hint: string; run: () => Promise<unknown> | void };
  /** The button for a typed time: "Snooze", "Set", "Schedule". */
  timeVerb: string;
  /** A time: the instant the dialog showed, and how the toast names it. */
  at: (until: string, label: string) => Promise<unknown> | void;
}

/** The untimed reply later: into the reply queue now. Undo from the toast. */
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

/** Snooze these messages until the instant shown. */
function snooze(messageIds: string[], until: string, label: string) {
  return performMailAction("snooze", messageIds, { until, payload: { untilLabel: label } });
}

/**
 * Mail and Focus: Later is the untimed reply later (one conversation, not
 * on Waiting on, which needs a time); a time snoozes. On the desk a time
 * takes the conversation off the desk instead, as `b` always did there.
 */
export function mailNotNow(
  target: MailTarget,
  subject: string,
  hooks: { onSnoozed?: () => void; onLater?: () => void; onDesk?: boolean } = {},
): NotNowPicker {
  const messageId = target.conversations === 1 ? target.primary?.id : undefined;
  const waiting = target.threadId
    ? deskLaneOf(target.threadId, target.accountId) === "waiting"
    : false;
  if (hooks.onDesk && target.threadId && target.conversations === 1) {
    return conversationNotNow({
      threadId: target.threadId,
      accountId: target.accountId,
      messageId,
      subject,
      waiting,
    });
  }
  return messageNotNow({
    messageIds: target.messageIds,
    replyLaterId: waiting ? undefined : messageId,
    subject,
    ...hooks,
  });
}

/** Now's lit row, or any one email: Later is the untimed reply later; a time snoozes. */
export function messageNotNow({
  messageIds,
  replyLaterId,
  subject,
  onSnoozed,
  onLater,
}: {
  messageIds: string[];
  /** The message the untimed reply later flags; none hides Later. */
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

/**
 * Messages: Later is the untimed reply later; a time takes the conversation
 * off the desk until then (`DeferThreads`, what `b` always sent). On
 * Waiting on a time brings it back only if nobody replied, and there is no
 * untimed choice.
 */
export function conversationNotNow({
  threadId,
  accountId,
  messageId,
  subject,
  waiting,
}: {
  threadId: string;
  /** The row's account: a thread id can be held by two accounts. */
  accountId?: string | null;
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
    at: (until, label) => deferThreadsAt([threadId], until, label, accountId),
  };
}

/** Reading's Later: on the shelf, or kept there when Later is the view. */
export const READING_LATER = {
  label: "Later",
  hint: "No time: it stays on your Later shelf",
} as const;
export const READING_KEEP = {
  label: "Keep it on Later",
  hint: "No time: it stays on the shelf",
} as const;

/**
 * Reading: Later puts the item on the Later shelf (the place says how); a
 * time snoozes the issue's email.
 */
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
