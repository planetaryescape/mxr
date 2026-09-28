/*
 * The end of a focus session: calm and specific. It names what comes next,
 * the soonest promise you made that is still open, so finishing points
 * somewhere instead of at an empty page. Working through the whole queue
 * earns the low tide scene in `data-slot="focus-finish-moment"`.
 */

import { Button } from "@/components/ui/button";
import { KeyChip } from "@/components/KeyChip";
import { LowTideScene } from "@/features/low-tide/LowTide";
import { dueDay, useNextPromise } from "@/features/promises/useNextPromise";
import { plural } from "@/lib/format";

export function FocusFinish({
  replied,
  empty,
  deferred,
  more,
  onRevisit,
  onContinue,
  onLeave,
}: {
  /** Conversations handled this session. */
  replied: number;
  /** Nobody was waiting when the session started. */
  empty: boolean;
  /** Skipped when nothing else was left. */
  deferred: number;
  /** Owed conversations the desk lane didn't return. */
  more: number;
  onRevisit: () => void;
  onContinue: () => void;
  onLeave: () => void;
}) {
  const next = useNextPromise();
  // Only claim everyone when nobody is left anywhere.
  const title =
    more > 0
      ? "That's this batch."
      : deferred > 0
        ? `${plural(deferred, "conversation")} skipped.`
        : empty
          ? "Nobody is waiting on a reply from you."
          : "Low tide. That's everyone.";
  // Earned only by working through the queue: a session that started empty
  // has nothing to celebrate.
  const cleared = more === 0 && deferred === 0 && !empty;
  return (
    <div
      data-testid="focus-finish"
      className="mx-auto flex max-w-[34rem] flex-1 flex-col justify-center px-6 py-16"
    >
      <div data-slot="focus-finish-moment" className="mb-6 empty:hidden">
        {cleared ? <LowTideScene sound /> : null}
      </div>
      <h2 className="text-balance text-2xl font-semibold tracking-tight">{title}</h2>
      <p className="mt-2 text-pretty text-[14px] leading-6 text-muted-foreground tabular-nums">
        {replied > 0 ? `${plural(replied, "conversation")} handled. ` : null}
        {more > 0 ? `${plural(more, "more conversation")} waiting on a reply. ` : null}
        {next ? (
          <>
            Next thing due:{" "}
            <span className="text-foreground" data-testid="focus-next-due">
              {dueDay(next.due)}, {next.commitment.what}
              {next.commitment.email ? ` to ${next.commitment.email}` : ""}
            </span>
            .
          </>
        ) : (
          "Nothing you promised is due."
        )}
      </p>
      <div className="mt-6 flex flex-wrap gap-2">
        {more > 0 ? (
          <Button size="sm" onClick={onContinue}>
            Continue with {more} more
          </Button>
        ) : null}
        {deferred > 0 ? (
          <Button size="sm" variant={more > 0 ? "outline" : "default"} onClick={onRevisit}>
            Come back to {deferred === 1 ? "it" : "them"}
          </Button>
        ) : null}
        <Button variant="outline" size="sm" onClick={onLeave} className="gap-2">
          Back to mail <KeyChip className="h-4 px-1">esc</KeyChip>
        </Button>
      </div>
    </div>
  );
}
