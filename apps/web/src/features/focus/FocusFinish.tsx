/*
 * The end of a focus session: calm and specific. It names what comes next,
 * the soonest promise you made that is still open, so finishing points
 * somewhere instead of at an empty page. Working through the whole queue
 * earns the low tide scene in `data-slot="focus-finish-moment"`.
 */

import { PageEmpty } from "@/components/PageParts";
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
  others,
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
  /** People who wrote to you but you don't write with: they wait in Messages. */
  others: number;
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
          ? "Nobody you write with is waiting on a reply."
          : "Low tide. That's everyone you write with.";
  // Earned only by working through the queue: a session that started empty
  // has nothing to celebrate.
  const cleared = more === 0 && deferred === 0 && !empty;
  return (
    <div data-testid="focus-finish" className="min-h-0 flex-1 overflow-y-auto">
      <PageEmpty
        framed
        moment={
          <div data-slot="focus-finish-moment" className="empty:hidden">
            {cleared ? <LowTideScene sound /> : null}
          </div>
        }
        title={title}
        body={
          <span className="tabular-nums">
            {replied > 0 ? `${plural(replied, "conversation")} handled. ` : null}
            {more > 0 ? `${plural(more, "more conversation")} waiting on a reply. ` : null}
            {others > 0
              ? `${plural(others, "other")} who wrote to you ${others === 1 ? "is" : "are"} in Messages. `
              : null}
            {next ? (
              <>
                Next thing due:{" "}
                <span className="text-text-1" data-testid="focus-next-due">
                  {dueDay(next.due)}, {next.commitment.what}
                  {next.commitment.email ? ` to ${next.commitment.email}` : ""}
                </span>
                .
              </>
            ) : (
              "Nothing you promised is due."
            )}
          </span>
        }
        action={
          <div className="flex flex-wrap gap-2">
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
        }
      />
    </div>
  );
}
