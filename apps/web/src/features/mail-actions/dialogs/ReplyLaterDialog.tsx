import { useRef } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { deferThreads } from "@/features/desk/deskLater";
import { setReplyLater } from "@/features/reply-queue/api";
import type { TimeChoice } from "@/features/time/api";
import { NaturalTimeInput } from "@/features/time/NaturalTimeInput";
import { useNaturalTime } from "@/features/time/useNaturalTime";

import { invalidateMailQueries } from "../mailQueryInvalidation";
import type { MailTarget } from "../target";
import { toneFor, VERB_FEEDBACK } from "../verbFeedback";

interface ReplyLaterDialogProps {
  target: MailTarget;
  subject: string;
  /** On a Waiting on row: the time brings it back if nobody replies. */
  waiting: boolean;
  onClose: () => void;
}

/**
 * Reply later from one key (`b`): type when ("tue 9", "in 2d"), see the
 * exact time before anything is stored, and the conversation comes back
 * then. On a thread you wrote last it comes back only if nobody replied.
 * Enter with no time is the plain reply later: the queue, now.
 */
export function ReplyLaterDialog({ target, subject, waiting, onClose }: ReplyLaterDialogProps) {
  const time = useNaturalTime();
  const inputRef = useRef<HTMLInputElement>(null);

  function setAt(choice: TimeChoice) {
    if (!target.threadId) return;
    onClose();
    time.reset();
    return deferThreads([target.threadId], choice).then(() => undefined);
  }

  /** No time: the untimed flag, as `b` always did. */
  async function untimed(): Promise<void> {
    const messageId = target.primary?.id;
    if (!messageId) return;
    onClose();
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

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          time.reset();
          onClose();
        }
      }}
    >
      <DialogContent
        className="max-w-md grid-cols-[minmax(0,1fr)]"
        // Capture, ahead of the field: its Enter would take the submission
        // guard with nothing to store. With nothing typed, Enter is the
        // plain reply later (not on Waiting on, which needs a time).
        onKeyDownCapture={(event) => {
          if (event.key !== "Enter" || waiting || time.value.trim()) return;
          if (!(event.target instanceof HTMLInputElement)) return;
          event.preventDefault();
          event.stopPropagation();
          if (!event.repeat) void time.runExclusive(untimed);
        }}
      >
        <DialogHeader>
          <DialogTitle>{waiting ? "Bring back if nobody replies" : "Reply later"}</DialogTitle>
          <DialogDescription className="truncate">
            {waiting
              ? `${subject} leaves Waiting on and comes back then, unless someone replies.`
              : `${subject} leaves the desk and comes back to You owe and your reply queue then.`}
          </DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-1.5"
          onSubmit={(event) => {
            event.preventDefault();
            if (!waiting && !time.value.trim()) {
              void time.runExclusive(untimed);
              return;
            }
            void time.commit(setAt);
          }}
        >
          <label htmlFor="reply-later-time" className="text-[13px] font-medium">
            {waiting ? "Bring it back" : "Back"}
          </label>
          <div className="flex items-start gap-2">
            <NaturalTimeInput
              id="reply-later-time"
              state={time}
              inputRef={inputRef}
              onCommit={setAt}
              autoFocus
              placeholder={waiting ? "in 3d, fri 3, next week" : "tue 9, tomorrow, in 2d"}
              hint={
                waiting
                  ? 'Try "in 3d", "fri 3" or "next week".'
                  : 'Try "tue 9", "tomorrow" or "in 2d". Enter with no time adds it to the reply queue now.'
              }
              className="min-w-0 flex-1"
            />
            <Button
              type="submit"
              size="sm"
              disabled={time.submitting || (waiting && !time.canCommit)}
            >
              Set
            </Button>
          </div>
        </form>
        <DialogFooter className="text-2xs text-muted-foreground sm:justify-start">
          Undo from the toast for about a minute afterwards.
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
