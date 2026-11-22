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
import { foldsFor, type LeaveVia } from "@/lib/leaving";
import { getActiveQueryClient } from "@/lib/queryClient";

import type { ModeKind } from "./membership";
import { MESSAGES_KEY } from "@/features/messages/api";

type Schemas = components["schemas"];
export type ModeDoneOutcome = Schemas["ModeDoneOutcomeData"];
type ModeDoneResponse = Extract<Schemas["ResponseData"], { kind: "ModeDone" }>;

export type DoneMode = Exclude<ModeKind, "archive">;

export type HiddenByMode = Record<DoneMode, ReadonlySet<string>>;

export const NONE_HIDDEN: HiddenByMode = {
  messages: new Set(),
  todo: new Set(),
  updates: new Set(),
  reading: new Set(),
};

interface ModeDoneState {
  hidden: HiddenByMode;
  folding: HiddenByMode;
  hide: (mode: DoneMode, threadIds: readonly string[]) => void;
  show: (mode: DoneMode, threadIds: readonly string[]) => void;
  fold: (mode: DoneMode, threadIds: readonly string[]) => void;
  settle: (mode: DoneMode, threadIds: readonly string[]) => void;
}

export const useModeDone = create<ModeDoneState>((set) => ({
  hidden: NONE_HIDDEN,
  folding: NONE_HIDDEN,
  fold: (mode, threadIds) =>
    set((s) => ({
      folding: { ...s.folding, [mode]: new Set([...s.folding[mode], ...threadIds]) },
    })),
  settle: (mode, threadIds) =>
    set((s) =>
      threadIds.some((id) => s.folding[mode].has(id))
        ? {
            folding: {
              ...s.folding,
              [mode]: new Set([...s.folding[mode]].filter((id) => !threadIds.includes(id))),
            },
          }
        : s,
    ),
  hide: (mode, threadIds) =>
    set((s) => ({
      hidden: { ...s.hidden, [mode]: new Set([...s.hidden[mode], ...threadIds]) },
    })),
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

export interface DoneScope {
  todoIds?: readonly string[];
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

export function doneToast(done: readonly ModeDoneOutcome[]): string {
  const [first] = done;
  if (!first) return "";
  if (done.length === 1) return first.copy;
  const same = done.every((item) => item.copy === first.copy);
  const count = plural(done.length, "conversation");
  return same ? `${count}: ${first.copy}` : `Done with ${count}.`;
}

export interface ModeDoneOptions extends DoneScope {
  onUndone?: () => void;
  via?: LeaveVia;
  message?: (done: readonly ModeDoneOutcome[]) => string;
}

const pendingTodoIds = new Set<string>();
const pendingThreads = new Map<string, number>();

export async function markModeDone(
  mode: DoneMode,
  threadIds: readonly string[],
  options: ModeDoneOptions = {},
): Promise<boolean> {
  const unique = [...new Set(threadIds)];
  if ((unique.length === 0 && !options.sender) || refuseWhileDaemonDown("mark it done")) {
    return false;
  }
  const inFlight = useModeDone.getState().hidden[mode];
  if (!options.todoIds?.length && unique.length > 0 && unique.every((id) => inFlight.has(id))) {
    return false;
  }
  const todoIds = options.todoIds?.length
    ? [...new Set(options.todoIds)].filter((id) => !pendingTodoIds.has(id))
    : undefined;
  if (todoIds?.length === 0) return false;
  todoIds?.forEach((id) => pendingTodoIds.add(id));
  for (const id of unique) {
    const key = `${mode}:${id}`;
    pendingThreads.set(key, (pendingThreads.get(key) ?? 0) + 1);
  }
  useModeDone.getState().hide(mode, unique);
  if (options.via && foldsFor(options.via)) useModeDone.getState().fold(mode, unique);
  const claim = claimUndo();
  try {
    const result = await doneModeRequest(
      mode,
      unique,
      false,
      todoIds ? { ...options, todoIds } : options,
    );
    const failed = result.items.filter((item) => item.error);
    const done = result.items.filter((item) => !item.error);
    const mutationId = result.mutation_id ?? null;
    const reverse = mutationId
      ? async () => {
          const undone = await performUndo(mutationId);
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
          "mode-done",
          (options.message ?? doneToast)(done),
          `mode-done-${mutationId ?? `${mode}-${unique.join(",")}`}`,
          reverse,
          claim.run,
          mutationId ?? undefined,
        ),
      );
      if (result.undo_unavailable) toast.warning("Done, but its undo couldn't be saved");
    } else {
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
    todoIds?.forEach((id) => pendingTodoIds.delete(id));
    const settled = unique.filter((id) => {
      const key = `${mode}:${id}`;
      const remaining = (pendingThreads.get(key) ?? 1) - 1;
      if (remaining > 0) {
        pendingThreads.set(key, remaining);
        return false;
      }
      pendingThreads.delete(key);
      return true;
    });
    useModeDone.getState().show(mode, settled);
    useModeDone.getState().settle(mode, settled);
  }
}

export async function refreshModes(): Promise<void> {
  const qc = getActiveQueryClient();
  await Promise.all([
    invalidateMailQueries(qc).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: ["rail"] }).catch(() => undefined),
    qc?.invalidateQueries({ queryKey: MESSAGES_KEY }).catch(() => undefined),
  ]);
}
