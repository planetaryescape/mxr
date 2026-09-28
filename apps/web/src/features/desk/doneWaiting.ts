/*
 * "Done waiting" on conversations you wrote last. Archive cannot take a
 * thread you started off the desk (there is nothing in the inbox to
 * archive), so on a Waiting row `e` marks it done in the daemon instead,
 * until a new message arrives. The row leaves at once and `u` brings it
 * back, like every other verb.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { apiFetch } from "@/api/client";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo, offerUndo } from "@/features/mail-actions/mailUndo";
import { VERB_FEEDBACK } from "@/features/mail-actions/verbFeedback";
import { plural } from "@/lib/format";
import { useSelection } from "@/state/selectionStore";

interface DoneWaitingState {
  /** Threads hidden while their dismissal is in flight. */
  hidden: ReadonlySet<string>;
  hide: (threadIds: string[]) => void;
  show: (threadIds: string[]) => void;
}

export const useDoneWaiting = create<DoneWaitingState>((set) => ({
  hidden: new Set(),
  hide: (threadIds) => set((s) => ({ hidden: new Set([...s.hidden, ...threadIds]) })),
  show: (threadIds) =>
    set((s) => ({ hidden: new Set([...s.hidden].filter((id) => !threadIds.includes(id))) })),
}));

function post(path: "dismiss" | "restore", threadIds: string[]) {
  return apiFetch<unknown>(`/api/v1/mail/desk/${path}`, {
    method: "POST",
    body: { thread_ids: threadIds },
  });
}

async function restore(threadIds: string[]): Promise<boolean> {
  try {
    await post("restore", threadIds);
    toast.success("Undone");
    await invalidateMailQueries();
    return true;
  } catch (error) {
    toast.error("Undo failed", {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  }
}

export async function markDoneWaiting(threadIds: string[]): Promise<void> {
  const ids = [...new Set(threadIds)];
  if (ids.length === 0) return;
  useDoneWaiting.getState().hide(ids);
  useSelection.getState().clear();
  // The row is gone at once, so `u` may come before the daemon answers:
  // hold the undo slot now, as every mail verb does.
  const claim = claimUndo();
  try {
    await post("dismiss", ids);
    claim.settle(
      offerUndo(
        `${VERB_FEEDBACK["done-waiting"].pastTense} ${plural(ids.length, "conversation")}`,
        `done-waiting-${ids.join(",")}`,
        () => restore(ids),
        claim.run,
      ),
    );
    await invalidateMailQueries().catch(() => undefined);
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't mark it done", {
      description: error instanceof Error ? error.message : String(error),
    });
  } finally {
    // The refetched desk now agrees (or the failure put the row back).
    useDoneWaiting.getState().show(ids);
  }
}
