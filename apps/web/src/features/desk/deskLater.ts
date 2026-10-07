/*
 * Reply later, or "bring it back if nobody replies", at a time: one daemon
 * request (`DeferThreads`) that decides by who wrote last. The client sends
 * the instant the time field previewed, never the words. The row leaves the
 * desk at once; the toast names the exact time, and `u` or its Undo puts
 * the flags and reminders back through the daemon's mutation undo.
 */

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

/** The toast for what the daemon set: it names the time exactly. */
export function deferredMessage(kinds: readonly DeferKind[], choice: TimeChoice): string {
  const when = describeChoice(choice);
  return kinds.length > 0 && kinds.every((kind) => kind === "waiting")
    ? `Back ${when} if nobody replies`
    : `Reply later: back ${when}`;
}

/**
 * The desk lane a conversation is in, from the desk already loaded, so a
 * dialog can say what its time will do. Unknown off the desk; the daemon
 * decides either way.
 */
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

/** Set the time. Resolves true when every conversation took it. */
export async function deferThreads(threadIds: string[], choice: TimeChoice): Promise<boolean> {
  const unique = [...new Set(threadIds)];
  if (unique.length === 0) return false;
  // Off the desk while the request runs (the hidden set Done uses).
  useDeskDone.getState().hide(unique);
  // The row is gone at once, so `u` may come before the daemon answers.
  const claim = claimUndo();
  try {
    const result = await apiFetch<ThreadsDeferred>("/api/v1/mail/desk/later", {
      method: "POST",
      body: { thread_ids: unique, until: choice.at },
    });
    const set = result.items.flatMap((item) => (item.error || !item.kind ? [] : [item.kind]));
    const failed = result.items.filter((item) => item.error);
    const mutationId = result.mutation_id;
    if (set.length > 0) {
      const reverse = mutationId ? () => performUndo(mutationId) : null;
      claim.settle(
        offerUndo(
          "reply-later-at",
          deferredMessage(set, choice),
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
