/*
 * Updates' verbs: let go of the digest or one source, this needs me, and
 * tune a source. Letting go is one `LetGoDigest`; its undo is the daemon's
 * by mutation id, as for any done. Tuning's undo sets the setting it had.
 * Lines fold up as they go, or drop at once under reduced motion.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { claimUndo, offerUndo, performUndo } from "@/features/mail-actions/mailUndo";
import { soundFor } from "@/features/mail-actions/verbFeedback";
import { refreshModes } from "@/features/modes/modeDone";
import { NOW_KEY } from "@/features/now/api";
import { playSound } from "@/features/sound/player";
import { makeTodo } from "@/features/todo/todoVerbs";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  letGoRequest,
  setSourceRequest,
  UPDATES_KEY,
  type LetGoInput,
  type UpdateLine,
  type UpdateSetting,
  type UpdatesLetGo,
} from "./api";
import { tuneToast } from "./digestView";

interface UpdatesHiddenState {
  /** Lines folding up: still drawn, animating out. */
  leaving: ReadonlySet<string>;
  /** Lines gone until the digest is refetched. */
  hidden: ReadonlySet<string>;
  leave: (ids: readonly string[]) => void;
  hide: (ids: readonly string[]) => void;
  show: (ids: readonly string[]) => void;
}

const without = (set: ReadonlySet<string>, ids: readonly string[]) =>
  new Set([...set].filter((id) => !ids.includes(id)));

export const useUpdatesHidden = create<UpdatesHiddenState>((set) => ({
  leaving: new Set(),
  hidden: new Set(),
  leave: (ids) => set((s) => ({ leaving: new Set([...s.leaving, ...ids]) })),
  hide: (ids) =>
    set((s) => ({ leaving: without(s.leaving, ids), hidden: new Set([...s.hidden, ...ids]) })),
  show: (ids) =>
    set((s) =>
      ids.some((id) => s.hidden.has(id) || s.leaving.has(id))
        ? { leaving: without(s.leaving, ids), hidden: without(s.hidden, ids) }
        : s,
    ),
}));

function motionReduced(): boolean {
  return document.documentElement.dataset.motion === "reduced";
}

/**
 * Fold lines up (the row hides itself when its fold ends); at once under
 * reduced motion.
 */
function startLeaving(ids: readonly string[]): void {
  const store = useUpdatesHidden.getState();
  if (motionReduced()) store.hide(ids);
  else store.leave(ids);
}

/** Refetch Updates and everything a let go or a new to-do can change. */
export async function refreshUpdates(): Promise<void> {
  const qc = getActiveQueryClient();
  await Promise.all([
    qc?.invalidateQueries({ queryKey: UPDATES_KEY }).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: NOW_KEY }).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: ["mode-guide"] }).catch(() => undefined),
    refreshModes(),
  ]);
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Let go for real, with the line ids that fold while it runs. The toast
 * is the daemon's own line ("Let go of 31 updates from 12 sources"), and
 * Undo reverses the whole run by its mutation id.
 */
export async function letGo(
  input: Omit<LetGoInput, "dryRun">,
  lineIds: readonly string[],
): Promise<UpdatesLetGo | null> {
  if (refuseWhileDaemonDown("let go")) return null;
  startLeaving(lineIds);
  const claim = claimUndo();
  try {
    const result = await letGoRequest({ ...input, dryRun: false });
    const mutationId = result.mutation_id ?? null;
    const reverse = mutationId
      ? async () => {
          const undone = await performUndo(mutationId);
          await refreshUpdates();
          return undone;
        }
      : null;
    if (result.message_count > 0) {
      const sound = soundFor("digest-let-go");
      if (sound) playSound(sound);
    }
    claim.settle(
      offerUndo(
        "digest-let-go",
        result.message_count > 0 ? result.line : "Nothing to let go of.",
        `updates-let-go-${mutationId ?? lineIds.join(",")}`,
        reverse,
        claim.run,
        mutationId ?? undefined,
      ),
    );
    const failed = result.items.filter((item) => item.error);
    if (failed.length > 0) {
      toast.error(`Couldn't let go of ${failed.length} of them`, {
        description: failed[0]?.error ?? undefined,
      });
    }
    if (result.undo_unavailable) toast.warning("Let go, but its undo couldn't be saved");
    return result;
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't let go", { description: errorText(error) });
    return null;
  } finally {
    await refreshUpdates();
    useUpdatesHidden.getState().show(lineIds);
  }
}

/**
 * `e`: let go of one source in the shown cut, after the same preview as
 * the whole digest, with the token that preview returned.
 */
export function letGoSource(line: UpdateLine, cut: string, selectionToken: string) {
  return letGo({ account: line.account_id, cut, sourceKey: line.source_key, selectionToken }, [
    line.id,
  ]);
}

/** `t`: this needs me. A to-do from the message the line's fact came from. */
export async function needsMe(line: UpdateLine): Promise<void> {
  const messageId = line.fact_message_id ?? line.latest_message_id;
  if (!messageId) {
    toast.info("This one has no email to make a to-do from");
    return;
  }
  await makeTodo({ messageId, title: line.todo_title });
  await refreshUpdates();
}

/** `K`: tune a source. Undo sets back the setting it had. */
export async function tuneSource(line: UpdateLine, setting: UpdateSetting): Promise<void> {
  if (refuseWhileDaemonDown("tune it")) return;
  const claim = claimUndo();
  try {
    const change = await setSourceRequest({
      accountId: line.account_id,
      source: line.source_key,
      setting,
    });
    const prior = change.prior;
    const reverse =
      prior === setting
        ? null
        : async () => {
            try {
              await setSourceRequest({
                accountId: line.account_id,
                source: line.source_key,
                setting: prior,
              });
              toast.success("Undone");
              return true;
            } catch (error) {
              toast.error("Undo failed", { description: errorText(error) });
              return false;
            } finally {
              await refreshUpdates();
            }
          };
    claim.settle(
      // Tuning moves a source's mail between digest and hidden, as moving a
      // sender moves it between places: the same toast tone and undo.
      offerUndo(
        "move-sender",
        tuneToast(line.source_name, setting),
        `updates-tune-${line.source_key}`,
        reverse,
        claim.run,
      ),
    );
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't tune it", { description: errorText(error) });
  } finally {
    await refreshUpdates();
  }
}
