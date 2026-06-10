/*
 * Daemon event types for the WebSocket stream.
 *
 * The bridge sends events typed as `IpcPayload::Event(DaemonEvent)`. We
 * keep a hand-rolled mirror here because the OpenAPI generator does not
 * surface the WebSocket message shape — only the HTTP envelope.
 *
 * Source: crates/protocol/src/types.rs (DaemonEvent enum). Update this
 * file when new variants land. If a variant name drifts, the discriminator
 * branches in `useDaemonEvents` will simply not fire — no runtime errors.
 */

export type DaemonEvent =
  | { type: "SyncCompleted"; event?: "SyncCompleted"; account_id: string; messages_synced: number }
  | { type: "SyncError"; event?: "SyncError"; account_id: string; error: string }
  | { type: "NewMessages"; event?: "NewMessages"; envelopes: Array<Record<string, unknown>> }
  | { type: "MessageUnsnoozed"; event?: "MessageUnsnoozed"; message_id: string }
  | { type: "ReminderTriggered"; event?: "ReminderTriggered"; sent_message_id: string }
  | { type: "LabelCountsUpdated"; event?: "LabelCountsUpdated"; counts: unknown[] }
  | {
      type: "OperationStarted";
      event?: "OperationStarted";
      operation_id: string;
      operation: string;
      account_id?: string | null;
      message: string;
    }
  | {
      type: "OperationProgress";
      event?: "OperationProgress";
      operation_id: string;
      operation: string;
      account_id?: string | null;
      current: number;
      total?: number | null;
      message: string;
    }
  | {
      type: "OperationCompleted";
      event?: "OperationCompleted";
      operation_id: string;
      operation: string;
      account_id?: string | null;
      message: string;
    }
  | {
      type: "OperationFailed";
      event?: "OperationFailed";
      operation_id: string;
      operation: string;
      account_id?: string | null;
      error: string;
      retryable: boolean;
    }
  | {
      type: "OperationCancelled";
      event?: "OperationCancelled";
      operation_id: string;
      operation: string;
      account_id?: string | null;
      message: string;
    }
  | {
      type: "MutationReconciliationFailed";
      event?: "MutationReconciliationFailed";
      client_correlation_id: string;
      error_summary: string;
    }
  | { type: string; [key: string]: unknown };

export type DaemonEventHandler = (event: DaemonEvent) => void;
