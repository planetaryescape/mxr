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
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { plural } from "@/lib/format";
import { getActiveQueryClient } from "@/lib/queryClient";

import type { ModeKind } from "./membership";

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

export function doneModeRequest(mode: DoneMode, threadIds: readonly string[], dryRun: boolean) {
  return apiFetch<ModeDoneResponse>(`/api/v1/mail/modes/${mode}/done`, {
    method: "POST",
    body: { thread_ids: threadIds, dry_run: dryRun },
  });
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

export interface ModeDoneOptions {
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
  if (unique.length === 0 || refuseWhileDaemonDown("mark it done")) return false;
  useModeDone.getState().hide(mode, unique);
  // The row is gone at once, so `u` may come before the daemon answers.
  const claim = claimUndo();
  try {
    const result = await doneModeRequest(mode, unique, false);
    const failed = result.items.filter((item) => item.error);
    const done = result.items.filter((item) => !item.error);
    const mutationId = result.mutation_id ?? null;
    const reverse = mutationId
      ? async () => {
          const undone = await performUndo(mutationId);
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
      claim.settle(null);
    }
    if (failed.length > 0) {
      toast.error(`Couldn't mark ${plural(failed.length, "conversation")} done`, {
        description: failed[0]?.error ?? undefined,
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
 * Every view a done can change: mail lists, Now, membership, To do and the
 * rail. The rail polls on its own otherwise, so ordinary mail verbs leave it.
 */
export async function refreshModes(): Promise<void> {
  const qc = getActiveQueryClient();
  await Promise.all([
    invalidateMailQueries(qc).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: ["todos"] }).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: ["rail"] }).catch(() => undefined),
  ]);
}
