/*
 * The end of a focus session: calm and specific. It names what comes next,
 * the soonest promise you made that is still open, so finishing points
 * somewhere instead of at an empty page. `data-slot="focus-finish-moment"`
 * is where a celebration can go later; nothing celebrates here yet.
 */

import { useQueries } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { KeyChip } from "@/components/KeyChip";
import { listCommitments } from "@/features/mailbox/api";
import type { Commitment } from "@/features/promises/api";
import { plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { useScopeAccountIds } from "./useFocusSession";

/** An open promise of yours with its due instant parsed. */
interface DuePromise {
  commitment: Commitment;
  due: number;
}

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
          : "That's everyone.";
  return (
    <div
      data-testid="focus-finish"
      className="mx-auto flex max-w-[34rem] flex-1 flex-col justify-center px-6 py-16"
    >
      <div data-slot="focus-finish-moment" />
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

/** The soonest open promise you made that has a date, across the scope. */
function useNextPromise(): DuePromise | null {
  const scope = useUiPrefs((s) => s.accountScope);
  const { ids } = useScopeAccountIds(scope);
  const lists = useQueries({
    queries: ids.map((accountId) => ({
      queryKey: ["commitments", accountId, "open"],
      queryFn: () => listCommitments({ accountId, status: "open" }),
    })),
  });
  const now = Date.now();
  const due: DuePromise[] = [];
  for (const commitment of lists.flatMap((list) => list.data?.commitments ?? [])) {
    if (commitment.direction !== "yours" || !commitment.by_when) continue;
    const at = Date.parse(commitment.by_when);
    if (at >= now) due.push({ commitment, due: at });
  }
  return due.toSorted((a, b) => a.due - b.due)[0] ?? null;
}

/** "Mon" within the week, "Mon 12 Oct" beyond it: the day is what matters. */
function dueDay(due: number): string {
  const at = new Date(due);
  const days = (at.getTime() - Date.now()) / 86_400_000;
  return at.toLocaleDateString(undefined, {
    weekday: "short",
    ...(days > 6 ? { day: "numeric", month: "short" } : {}),
  });
}
