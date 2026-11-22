import { useQueries } from "@tanstack/react-query";

import { useScopeAccountIds } from "@/features/focus/useFocusSession";
import { listCommitments } from "@/features/mailbox/api";
import type { Commitment } from "@/features/promises/api";
import { useUiPrefs } from "@/state/uiPrefsStore";

export interface DuePromise {
  commitment: Commitment;
  due: number;
}

export function useNextPromise(): DuePromise | null {
  const scope = useUiPrefs((s) => s.accountScope);
  const { ids } = useScopeAccountIds(scope);
  const lists = useQueries({
    queries: ids.map((accountId) => ({
      queryKey: ["commitments", accountId, "open"],
      queryFn: () => listCommitments({ accountId, status: "open" }),
    })),
  });
  return nextDuePromise(
    lists.flatMap((list) => list.data?.commitments ?? []),
    Date.now(),
  );
}

export function nextDuePromise(commitments: Commitment[], now: number): DuePromise | null {
  const due: DuePromise[] = [];
  for (const commitment of commitments) {
    if (commitment.direction !== "yours" || !commitment.by_when || commitment.date_unverified) {
      continue;
    }
    const at = Date.parse(commitment.by_when);
    if (at >= now) due.push({ commitment, due: at });
  }
  return due.toSorted((a, b) => a.due - b.due)[0] ?? null;
}

export function dueDay(due: number): string {
  const at = new Date(due);
  const days = (at.getTime() - Date.now()) / 86_400_000;
  return at.toLocaleDateString(undefined, {
    weekday: "short",
    ...(days > 6 ? { day: "numeric", month: "short" } : {}),
  });
}
