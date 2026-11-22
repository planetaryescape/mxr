import type { components } from "./generated";

export type DaemonEvent =
  | { type: "ServerStopping"; event?: "ServerStopping"; boot_id: string; deadline_at: number }
  | { type: "SyncCompleted"; event?: "SyncCompleted"; account_id: string; messages_synced: number }
  | { type: "SyncError"; event?: "SyncError"; account_id: string; error: string }
  | {
      type: "NewMessages";
      event?: "NewMessages";
      envelopes: Array<Record<string, unknown>>;
      total?: number;
    }
  | { type: "MessageUnsnoozed"; event?: "MessageUnsnoozed"; message_id: string }
  | { type: "ReminderTriggered"; event?: "ReminderTriggered"; sent_message_id: string }
  | { type: "ReplyLaterReturned"; event?: "ReplyLaterReturned"; message_id: string }
  | { type: "LabelCountsUpdated"; event?: "LabelCountsUpdated"; counts: unknown[] }
  | { type: "ModesChanged"; event?: "ModesChanged"; account_id?: string | null }
  | { type: "AccountAddressesChanged"; event?: "AccountAddressesChanged"; account_id: string }
  | { type: "ContactsRefreshed"; event?: "ContactsRefreshed" }
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
  | { type: "EventsLagged"; event?: "EventsLagged"; skipped: number }
  | {
      type: "ThreadGistReady";
      event?: "ThreadGistReady";
      gist: components["schemas"]["ThreadGistData"];
    }
  | {
      type: "AssistantUpdated";
      event?: "AssistantUpdated";
      conversation_id: string;
      status: components["schemas"]["AssistantStatusData"];
      progress?: components["schemas"]["AssistantProgressData"] | null;
    }
  | { type: string; [key: string]: unknown };

export type DaemonEventHandler = (event: DaemonEvent) => void;
