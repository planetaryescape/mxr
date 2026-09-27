/*
 * The end of a focus session: calm and specific. It names what comes next,
 * the soonest promise you made that is still open, so finishing points
 * somewhere instead of at an empty page. `data-slot="focus-finish-moment"`
 * is where a celebration can go later; nothing celebrates here yet.
 */

import { useQueries, useQuery } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { KeyChip } from "@/components/KeyChip";
import { fetchAccounts } from "@/features/accounts/api";
import { listCommitments } from "@/features/mailbox/api";
import type { Commitment } from "@/features/promises/api";
import { plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

/** An open promise of yours with its due instant parsed. */
interface DuePromise {
  commitment: Commitment;
  due: number;
}

export function FocusFinish({
  replied,
  empty,
  onLeave,
}: {
  /** Conversations handled this session. */
  replied: number;
  /** Nobody was waiting when the session started. */
  empty: boolean;
  onLeave: () => void;
}) {
  const next = useNextPromise();
  return (
    <div
      data-testid="focus-finish"
      className="mx-auto flex max-w-[34rem] flex-1 flex-col justify-center px-6 py-16"
    >
      <div data-slot="focus-finish-moment" />
      <h2 className="text-balance text-2xl font-semibold tracking-tight">
        {empty ? "Nobody is waiting on a reply from you." : "That's everyone."}
      </h2>
      <p className="mt-2 text-pretty text-[14px] leading-6 text-muted-foreground tabular-nums">
        {empty ? null : `${plural(replied, "conversation")} handled. `}
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
      <div className="mt-6">
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
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts, staleTime: 60_000 });
  const ids = scope
    ? [scope]
    : (accounts.data?.accounts ?? []).map((account) => account.account_id);
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
