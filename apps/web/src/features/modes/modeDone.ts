/*
 * Done here, per mode (`SetModeDone`): the thread leaves this mode until a
 * new message arrives, other modes keep it, and the provider archive
 * happens only when no mode holds it any more. The toast is the daemon's
 * handoff copy ("Done in Messages. Still in To do (due Mon).", "Done.
 * Archived in Gmail."), so it always says where the item went, and `u` or
 * the toast's Undo puts everything back through the daemon's undo.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo, offerUndo, performUndo } from "@/features/mail-actions/mailUndo";
import { soundFor } from "@/features/mail-actions/verbFeedback";
import { playSound } from "@/features/sound/player";
import { useUndo } from "@/state/undoStore";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { plural } from "@/lib/format";
import { getActiveQueryClient } from "@/lib/queryClient";

import type { ModeKind } from "./membership";
import { MESSAGES_KEY } from "@/features/messages/api";

type Schemas = components["schemas"];
export type ModeDoneOutcome = Schemas["ModeDoneOutcomeData"];
type ModeDoneResponse = Extract<Schemas["ResponseData"], { kind: "ModeDone" }>;

/** Modes that have a done: Archive keeps its records. */
export type DoneMode = Exclude<ModeKind, "archive">;

/** Threads hidden while their done is in flight, per mode. */
export type HiddenByMode = Record<DoneMode, ReadonlySet<string>>;

export const NONE_HIDDEN: HiddenByMode = {
  messages: new Set(),
  todo: new Set(),
  updates: new Set(),
  reading: new Set(),
};

interface ModeDoneState {
  hidden: HiddenByMode;
  hide: (mode: DoneMode, threadIds: readonly string[]) => void;
  show: (mode: DoneMode, threadIds: readonly string[]) => void;
}

export const useModeDone = create<ModeDoneState>((set) => ({
  hidden: NONE_HIDDEN,
  hide: (mode, threadIds) =>
    set((s) => ({
      hidden: { ...s.hidden, [mode]: new Set([...s.hidden[mode], ...threadIds]) },
    })),
  // Unchanged state when none of them were hidden, so nothing re-renders.
  show: (mode, threadIds) =>
    set((s) =>
      threadIds.some((id) => s.hidden[mode].has(id))
        ? {
            hidden: {
              ...s.hidden,
              [mode]: new Set([...s.hidden[mode]].filter((id) => !threadIds.includes(id))),
            },
          }
        : s,
    ),
}));

/** Who a done covers besides its threads: named to-dos, or a sender's threads. */
export interface DoneScope {
  /** To do: tick off only these rows. */
  todoIds?: readonly string[];
  /** Updates or Reading: every thread of this sender's, resolved by the daemon. */
  sender?: { account_id: string; sender_email: string };
}

export function doneModeRequest(
  mode: DoneMode,
  threadIds: readonly string[],
  dryRun: boolean,
  scope: DoneScope = {},
) {
  return apiFetch<ModeDoneResponse>(`/api/v1/mail/modes/${mode}/done`, {
    method: "POST",
    body: {
      thread_ids: threadIds,
      dry_run: dryRun,
      ...(scope.todoIds?.length ? { todo_ids: scope.todoIds } : {}),
      ...(scope.sender ? { sender: scope.sender } : {}),
    },
  });
}

/**
 * Hand the claimed undo slot to `reverse`, when there is one: the slot
 * otherwise empties when the claim settles.
 */
function keepUndo(
  reverse: (() => Promise<boolean>) | null,
  claim: () => Promise<boolean>,
  mutationId: string | null,
): (() => Promise<boolean>) | null {
  if (!reverse) return null;
  const undo = async () => {
    useUndo.getState().retireUndo(undo);
    return reverse();
  };
  if (useUndo.getState().lastUndo === claim) {
    useUndo.getState().recordUndo(undo, mutationId ?? undefined);
  }
  return undo;
}

/**
 * What the toast says. One thread: the daemon's copy. Several: how many,
 * then the first thread's copy when they all went the same way.
 */
export function doneToast(done: readonly ModeDoneOutcome[]): string {
  const [first] = done;
  if (!first) return "";
  if (done.length === 1) return first.copy;
  const same = done.every((item) => item.copy === first.copy);
  const count = plural(done.length, "conversation");
  return same ? `${count}: ${first.copy}` : `Done with ${count}.`;
}

export interface ModeDoneOptions extends DoneScope {
  /** Runs after an undo of this done succeeded. */
  onUndone?: () => void;
  /** The toast, when the verb has its own words (letting go of a digest). */
  message?: (done: readonly ModeDoneOutcome[]) => string;
}

/**
 * Done here for these threads. Resolves true when every one was done; one
 * that couldn't be keeps its row (the refetch brings it back), and doing it
 * again retries it.
 */
export async function markModeDone(
  mode: DoneMode,
  threadIds: readonly string[],
  options: ModeDoneOptions = {},
): Promise<boolean> {
  const unique = [...new Set(threadIds)];
  if ((unique.length === 0 && !options.sender) || refuseWhileDaemonDown("mark it done")) {
    return false;
  }
  useModeDone.getState().hide(mode, unique);
  // The row is gone at once, so `u` may come before the daemon answers.
  const claim = claimUndo();
  try {
    const result = await doneModeRequest(mode, unique, false, options);
    const failed = result.items.filter((item) => item.error);
    const done = result.items.filter((item) => !item.error);
    const mutationId = result.mutation_id ?? null;
    const reverse = mutationId
      ? async () => {
          const undone = await performUndo(mutationId);
          // The undo puts to-dos and done marks back too, which the mail
          // refresh alone doesn't cover.
          await refreshModes();
          if (undone) options.onUndone?.();
          return undone;
        }
      : null;
    if (done.length > 0) {
      const sound = soundFor("mode-done");
      if (sound) playSound(sound);
      claim.settle(
        offerUndo(
          (options.message ?? doneToast)(done),
          `mode-done-${mutationId ?? `${mode}-${unique.join(",")}`}`,
          reverse,
          claim.run,
          mutationId ?? undefined,
        ),
      );
      if (result.undo_unavailable) toast.warning("Done, but its undo couldn't be saved");
    } else {
      // Nothing was done, but something may have changed part way (read,
      // not archived): the daemon kept an undo for it, so `u` and the
      // failure toast reach it.
      claim.settle(keepUndo(reverse, claim.run, mutationId));
    }
    if (failed.length > 0) {
      toast.error(`Couldn't mark ${plural(failed.length, "conversation")} done`, {
        description: failed[0]?.error ?? undefined,
        action:
          done.length === 0 && reverse
            ? { label: "Undo", onClick: () => void reverse() }
            : undefined,
      });
    }
    await refreshModes();
    return failed.length === 0;
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't mark it done", {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  } finally {
    useModeDone.getState().show(mode, unique);
  }
}

/**
 * Every view a done can change: mail lists, Now, membership, To do,
 * Messages and the rail. The rail polls on its own otherwise, so ordinary mail verbs leave it.
 */
export async function refreshModes(): Promise<void> {
  const qc = getActiveQueryClient();
  await Promise.all([
    invalidateMailQueries(qc).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: ["todos"] }).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: ["rail"] }).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: MESSAGES_KEY }).catch(() => undefined),
  ]);
}
