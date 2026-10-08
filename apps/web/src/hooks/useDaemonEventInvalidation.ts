import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";

import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { shellKey } from "@/features/mailbox/api";
import { useDaemonEvents } from "@/hooks/useDaemonEvents";
import { refetchAfterGap } from "@/lib/daemonAvailability";
import { useConnectionStore, type SyncProgress } from "@/state/connectionStore";

const CLEAR_SYNC_PROGRESS_DELAY_MS = 1_000;
let clearSyncProgressHandle: ReturnType<typeof setTimeout> | undefined;

export function useDaemonEventInvalidation(): void {
  const qc = useQueryClient();
  useDaemonEvents(
    useCallback(
      (event) => {
        // Event names are the daemon's DaemonEvent variants
        // (crates/protocol/src/types.rs); mail views all refresh through
        // the same invalidation the mutation layer uses.
        switch (event.type) {
          case "NewMessages":
          case "MessageUnsnoozed":
          // A move in any client: every mode, Now's line and Inbox's chips.
          case "ModesChanged":
            void invalidateMailQueries(qc);
            break;
          case "LabelCountsUpdated":
            void qc.invalidateQueries({ queryKey: shellKey });
            break;
          case "OperationStarted":
          case "OperationProgress":
            if (isSyncOperationEvent(event)) {
              setSyncProgress({
                account_id: event.account_id ?? "all",
                current: event.current ?? 0,
                total: event.total ?? event.current ?? 1,
              });
            }
            break;
          case "SyncCompleted":
          case "OperationCompleted":
          case "OperationFailed":
          case "OperationCancelled":
            if (event.type !== "SyncCompleted" && !isSyncOperationEvent(event)) break;
            clearSyncProgressSoon();
            void invalidateMailQueries(qc);
            break;
          case "SyncError":
            // A sync failed. Stop any sync-progress spinner. The failure
            // itself shows through freshness (the status bar, the pill and
            // the accounts), which toasts only when an account's health
            // changes, never on every retry of a failing account.
            clearSyncProgressSoon();
            void qc.invalidateQueries({ queryKey: shellKey });
            break;
          case "ReminderTriggered":
          case "ReplyLaterReturned":
            // A time the user set came: nobody replied, or reply later is
            // due. The conversation is back on the desk and in the reply
            // queue.
            void invalidateMailQueries(qc);
            break;
          case "MutationReconciliationFailed":
            // Optimistic UI rollback hint: the provider/store rejected a
            // mutation we already reflected locally. Refetch the affected
            // surfaces so the UI converges back to server truth, and tell
            // the user the action didn't stick.
            void invalidateMailQueries(qc);
            if (isReconciliationFailedEvent(event)) {
              toast.error(`Action didn't stick: ${event.error_summary}`);
            }
            break;
          case "EventsLagged":
            // The daemon dropped events for this client during a burst, so
            // any cached view could be stale. Invalidate everything (bar open
            // drafts) and let React Query refetch what's on screen:
            // correctness over a targeted diff we can't compute from missed
            // events.
            void refetchAfterGap(qc);
            break;
        }
      },
      [qc],
    ),
  );
}

/**
 * A sync that has said nothing for this long has ended without telling us
 * (a dropped frame, a daemon restart): stop showing it as running, or every
 * later "Sync now" is refused as already running.
 */
const SYNC_PROGRESS_STALE_MS = 60_000;

function setSyncProgress(syncProgress: SyncProgress): void {
  if (clearSyncProgressHandle) clearTimeout(clearSyncProgressHandle);
  clearSyncProgressHandle = setTimeout(() => {
    useConnectionStore.getState().setState({ syncProgress: undefined });
    clearSyncProgressHandle = undefined;
  }, SYNC_PROGRESS_STALE_MS);
  useConnectionStore.getState().setState({ syncProgress });
}

function clearSyncProgressSoon(): void {
  if (clearSyncProgressHandle) clearTimeout(clearSyncProgressHandle);
  clearSyncProgressHandle = setTimeout(() => {
    useConnectionStore.getState().setState({ syncProgress: undefined });
    clearSyncProgressHandle = undefined;
  }, CLEAR_SYNC_PROGRESS_DELAY_MS);
}

function isSyncOperationEvent(event: unknown): event is {
  operation: string;
  account_id?: string | null;
  current?: number;
  total?: number | null;
} {
  if (typeof event !== "object" || event === null) return false;
  const candidate = event as Record<string, unknown>;
  return candidate.operation === "sync";
}

function isReconciliationFailedEvent(
  event: unknown,
): event is { client_correlation_id: string; error_summary: string } {
  if (typeof event !== "object" || event === null) return false;
  const candidate = event as Record<string, unknown>;
  return typeof candidate.error_summary === "string";
}
