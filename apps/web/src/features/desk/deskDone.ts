/*
 * Done on the desk: "nothing for me to do here, put it away". One daemon
 * request per press (`ResolveDeskItems`): You owe and New from people are
 * archived, marked read and kept off the desk until someone writes again;
 * Waiting on is marked read and done waiting; a Due promise is resolved.
 * The row leaves at once, and `u` or the toast's Undo puts everything back
 * through the daemon's mutation undo.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo, offerUndo, performUndo } from "@/features/mail-actions/mailUndo";
import { soundFor, VERB_FEEDBACK } from "@/features/mail-actions/verbFeedback";
import { playSound } from "@/features/sound/player";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { plural } from "@/lib/format";
import { useSelection } from "@/state/selectionStore";

import type { DeskRow } from "./api";

type Schemas = components["schemas"];
export type DeskDoneItem = Schemas["DeskDoneItemData"];
type DeskDoneResponse = Extract<Schemas["ResponseData"], { kind: "DeskItemsResolved" }>;

interface DeskDoneState {
  /** Conversations hidden while their Done is in flight. */
  hidden: ReadonlySet<string>;
  hide: (threadIds: string[]) => void;
  show: (threadIds: string[]) => void;
}

export const useDeskDone = create<DeskDoneState>((set) => ({
  hidden: new Set(),
  hide: (threadIds) => set((s) => ({ hidden: new Set([...s.hidden, ...threadIds]) })),
  // Unchanged state when none of them were hidden, so nothing re-renders.
  show: (threadIds) =>
    set((s) =>
      threadIds.some((id) => s.hidden.has(id))
        ? { hidden: new Set([...s.hidden].filter((id) => !threadIds.includes(id))) }
        : s,
    ),
}));

/** What Done needs from a desk row: its conversation, lane and promise. */
export function deskDoneItem(row: DeskRow): DeskDoneItem {
  return {
    thread_id: row.thread_id,
    lane: row.lane,
    ...(row.commitment_id ? { commitment_id: row.commitment_id } : {}),
  };
}

export interface DeskDoneOptions {
  /** Runs after an undo of this Done succeeded (focus mode puts it back). */
  onUndone?: () => void;
}

/**
 * Put the items away. Resolves true when every one of them was done; a
 * conversation that could not be keeps its row (the refetch brings it
 * back) and pressing Done again retries it.
 */
export async function markDeskDone(
  items: readonly DeskDoneItem[],
  options: DeskDoneOptions = {},
): Promise<boolean> {
  const unique = [...new Map(items.map((item) => [item.thread_id, item])).values()];
  if (unique.length === 0 || refuseWhileDaemonDown("mark it done")) return false;
  const threadIds = unique.map((item) => item.thread_id);
  useDeskDone.getState().hide(threadIds);
  if (useSelection.getState().ids.size > 0) useSelection.getState().clear();
  // The row is gone at once, so `u` may come before the daemon answers:
  // hold the undo slot now, as every mail verb does.
  const claim = claimUndo();
  try {
    const result = await apiFetch<DeskDoneResponse>("/api/v1/mail/desk/done", {
      method: "POST",
      body: { items: unique },
    });
    const failed = result.items.filter((item) => item.error);
    const done = result.items.length - failed.length;
    const mutationId = result.mutation_id;
    const reverse = mutationId
      ? async () => {
          const undone = await performUndo(mutationId);
          if (undone) options.onUndone?.();
          return undone;
        }
      : null;
    if (done > 0) {
      // One sound per Done, however many conversations it put away.
      const sound = soundFor("desk-done");
      if (sound) playSound(sound);
      const past = VERB_FEEDBACK["desk-done"].pastTense;
      claim.settle(
        offerUndo(
          done === 1 ? past : `${past} with ${plural(done, "conversation")}`,
          `desk-done-${mutationId ?? threadIds.join(",")}`,
          reverse,
          claim.run,
          mutationId ?? undefined,
        ),
      );
      if (result.undo_unavailable) toast.warning("Done, but its undo couldn't be saved");
    } else {
      claim.settle(null);
    }
    if (failed.length > 0) {
      toast.error(`Couldn't put ${plural(failed.length, "conversation")} away`, {
        description: failed[0]?.error ?? undefined,
      });
    }
    await invalidateMailQueries().catch(() => undefined);
    return failed.length === 0;
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't mark it done", {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  } finally {
    // The refetched desk now agrees (or a failure put the row back).
    useDeskDone.getState().show(threadIds);
  }
}
