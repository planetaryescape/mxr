import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

import { shellKey } from "@/features/mailbox/api";
import { useDaemonEvents } from "@/hooks/useDaemonEvents";
import { useConnectionStore, type SyncProgress } from "@/state/connectionStore";

const CLEAR_SYNC_PROGRESS_DELAY_MS = 1_000;
let clearSyncProgressHandle: ReturnType<typeof setTimeout> | undefined;

export function useDaemonEventInvalidation(): void {
  const qc = useQueryClient();
  useDaemonEvents(
    useCallback(
      (event) => {
        switch (event.type) {
          case "NewMessages":
          case "MailUpdated":
          case "MailRemoved":
          case "MessageUnsnoozed":
            void qc.invalidateQueries({ queryKey: ["mailbox"] });
            void qc.invalidateQueries({ queryKey: ["thread"] });
            void qc.invalidateQueries({ queryKey: ["search"] });
            void qc.invalidateQueries({ queryKey: ["search-palette"] });
            void qc.invalidateQueries({ queryKey: shellKey });
            break;
          case "LabelCountsUpdated":
            void qc.invalidateQueries({ queryKey: shellKey });
            break;
          case "SyncProgress":
            if (isSyncProgressEvent(event)) {
              setSyncProgress({
                account_id: event.account_id,
                current: event.current,
                total: event.total,
              });
            }
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
            void qc.invalidateQueries({ queryKey: ["mailbox"] });
            void qc.invalidateQueries({ queryKey: ["search"] });
            void qc.invalidateQueries({ queryKey: shellKey });
            break;
        }
      },
      [qc],
    ),
  );
}

function setSyncProgress(syncProgress: SyncProgress): void {
  if (clearSyncProgressHandle) {
    clearTimeout(clearSyncProgressHandle);
    clearSyncProgressHandle = undefined;
  }
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

function isSyncProgressEvent(
  event: unknown,
): event is { account_id: string; current: number; total: number } {
  if (typeof event !== "object" || event === null) return false;
  const candidate = event as Record<string, unknown>;
  return (
    typeof candidate.account_id === "string" &&
    typeof candidate.current === "number" &&
    typeof candidate.total === "number"
  );
}
