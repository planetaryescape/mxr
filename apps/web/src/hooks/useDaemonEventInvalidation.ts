import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";

import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { shellKey } from "@/features/mailbox/api";
import { useDaemonEvents } from "@/hooks/useDaemonEvents";
import { refetchAfterGap } from "@/lib/daemonAvailability";
import { useConnectionStore, type SyncProgress } from "@/state/connectionStore";
import { useLanding } from "@/state/landingStore";

const CLEAR_SYNC_PROGRESS_DELAY_MS = 1_000;
let clearSyncProgressHandle: ReturnType<typeof setTimeout> | undefined;

export function useDaemonEventInvalidation(): void {
  const qc = useQueryClient();
  useDaemonEvents(
    useCallback(
      (event) => {
        if ("operation" in event && event.operation === "send_recovery") {
          void qc.invalidateQueries({ queryKey: ["send-outcome"] });
          void qc.invalidateQueries({ queryKey: ["drafts"] });
          const id = `send-recovery:${event.operation_id}`;
          if (event.type === "OperationCompleted") {
            toast.success("Mail provider accepted the approved send", { id });
            void invalidateMailQueries(qc);
          } else if (event.type === "OperationFailed") {
            toast.error("Send needs attention", {
              id,
              description:
                typeof event.error === "string"
                  ? event.error
                  : "Check Send outcomes in Drafts. Do not resend an uncertain operation.",
            });
          } else if (event.type === "OperationProgress" && typeof event.message === "string") {
            toast.info("Waiting for the mail provider", { id, description: event.message });
          }
          return;
        }
        switch (event.type) {
          case "NewMessages":
            useLanding.getState().land(arrivedIds(event.envelopes));
            void invalidateMailQueries(qc);
            break;
          case "MessageUnsnoozed":
          case "ModesChanged":
            void invalidateMailQueries(qc);
            break;
          case "AccountAddressesChanged":
            void invalidateMailQueries(qc);
            void qc.invalidateQueries({ queryKey: ["analytics"] });
            break;
          case "ContactsRefreshed":
            void qc.invalidateQueries({ queryKey: ["analytics"] });
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
            clearSyncProgressSoon();
            void qc.invalidateQueries({ queryKey: shellKey });
            break;
          case "ReminderTriggered":
          case "ReplyLaterReturned":
            void invalidateMailQueries(qc);
            break;
          case "MutationReconciliationFailed":
            void invalidateMailQueries(qc);
            if (isReconciliationFailedEvent(event)) {
              toast.error(`Action didn't stick: ${event.error_summary}`);
            }
            break;
          case "EventsLagged":
            void refetchAfterGap(qc);
            break;
        }
      },
      [qc],
    ),
  );
}

function arrivedIds(envelopes: unknown): string[] {
  if (!Array.isArray(envelopes)) return [];
  return (envelopes as ReadonlyArray<Record<string, unknown>>)
    .flatMap((envelope) => [envelope.id, envelope.thread_id])
    .filter((id): id is string => typeof id === "string");
}

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
