/*
 * The next thing you promised: the soonest open promise of yours that has
 * a date. The end of a focus session and a cleared desk both point at it,
 * so finishing points somewhere instead of at an empty page.
 */

import { useQueries } from "@tanstack/react-query";

import { useScopeAccountIds } from "@/features/focus/useFocusSession";
import { listCommitments } from "@/features/mailbox/api";
import type { Commitment } from "@/features/promises/api";
import { useUiPrefs } from "@/state/uiPrefsStore";

/** An open promise of yours with its due instant parsed. */
export interface DuePromise {
  commitment: Commitment;
  due: number;
}

/** The soonest open promise you made that has a date, across the scope. */
export function useNextPromise(): DuePromise | null {
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
export function dueDay(due: number): string {
  const at = new Date(due);
  const days = (at.getTime() - Date.now()) / 86_400_000;
  return at.toLocaleDateString(undefined, {
    weekday: "short",
    ...(days > 6 ? { day: "numeric", month: "short" } : {}),
  });
}
