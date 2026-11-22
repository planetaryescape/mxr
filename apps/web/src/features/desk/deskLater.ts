import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo, offerUndo, performUndo } from "@/features/mail-actions/mailUndo";
import { describeChoice, type TimeChoice } from "@/features/time/api";
import { plural } from "@/lib/format";
import { getActiveQueryClient } from "@/lib/queryClient";

import type { Desk, DeskLaneKind } from "./api";
import { useDeskDone } from "./deskDone";

type Schemas = components["schemas"];
type ThreadsDeferred = Extract<Schemas["ResponseData"], { kind: "ThreadsDeferred" }>;
export type DeferKind = Schemas["DeferKindData"];

export function deferredMessage(kinds: readonly DeferKind[], choice: TimeChoice): string {
  return deferredMessageAt(kinds, describeChoice(choice));
}

function deferredMessageAt(kinds: readonly DeferKind[], when: string): string {
  return kinds.length > 0 && kinds.every((kind) => kind === "waiting")
    ? `Back ${when} if nobody replies`
    : `Reply later: back ${when}`;
}

export function deskLaneOf(threadId: string): DeskLaneKind | undefined {
  const queries = getActiveQueryClient()?.getQueriesData<Desk>({ queryKey: ["desk"] }) ?? [];
  for (const [, desk] of queries) {
    if (!desk) continue;
    for (const lane of ["owed", "due", "waiting", "people_new"] as const) {
      if (desk[lane].rows.some((row) => row.thread_id === threadId)) return lane;
    }
  }
  return undefined;
}

export function deferThreads(threadIds: string[], choice: TimeChoice): Promise<boolean> {
  return deferThreadsAt(threadIds, choice.at, describeChoice(choice));
}

export async function deferThreadsAt(
  threadIds: string[],
  until: string,
  when: string,
): Promise<boolean> {
  const unique = [...new Set(threadIds)];
  if (unique.length === 0) return false;
  useDeskDone.getState().hide(unique);
  const claim = claimUndo();
  try {
    const result = await apiFetch<ThreadsDeferred>("/api/v1/mail/desk/later", {
      method: "POST",
      body: { thread_ids: unique, until },
    });
    const set = result.items.flatMap((item) => (item.error || !item.kind ? [] : [item.kind]));
    const failed = result.items.filter((item) => item.error);
    const mutationId = result.mutation_id;
    if (set.length > 0) {
      const reverse = mutationId ? () => performUndo(mutationId) : null;
      claim.settle(
        offerUndo(
          "reply-later-at",
          deferredMessageAt(set, when),
          `desk-later-${mutationId ?? unique.join(",")}`,
          reverse,
          claim.run,
          mutationId ?? undefined,
        ),
      );
      if (result.undo_unavailable) toast.warning("The time is set, but its undo couldn't be saved");
    } else {
      claim.settle(null);
    }
    if (failed.length > 0) {
      toast.error(`Couldn't set a time on ${plural(failed.length, "conversation")}`, {
        description: failed[0]?.error ?? undefined,
      });
    }
    await invalidateMailQueries().catch(() => undefined);
    return failed.length === 0;
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't set the time", {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  } finally {
    useDeskDone.getState().show(unique);
  }
}
