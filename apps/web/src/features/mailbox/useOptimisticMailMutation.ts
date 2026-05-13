import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  archiveMessages,
  markReadMessages,
  modifyLabels,
  moveMessagesToLabel,
  readAndArchiveMessages,
  shellKey,
  spamMessages,
  starMessages,
  trashMessages,
  undoMutation,
} from "./api";
import type { MailboxResponse, MessageGroupView, MutationResponse } from "./types";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { useSelection } from "@/state/selectionStore";

export type MailAction =
  | "archive"
  | "trash"
  | "spam"
  | "star"
  | "unstar"
  | "read"
  | "unread"
  | "read-and-archive"
  | "move"
  | "label-add"
  | "label-remove";

export interface MailActionPayload {
  /** Required for label/move actions — the label name being added, removed, or moved into. */
  label?: string;
}

interface MutationContext {
  snapshots: Array<[readonly unknown[], unknown]>;
}

const destructiveActions = new Set<MailAction>([
  "archive",
  "trash",
  "spam",
  "read-and-archive",
  "move",
  "label-remove",
]);

function isMailboxResponse(value: unknown): value is MailboxResponse {
  return typeof value === "object" && value !== null && "mailbox" in value;
}

function mapMailboxRows(
  data: MailboxResponse,
  ids: Set<string>,
  action: MailAction,
): MailboxResponse {
  const groups = data.mailbox.groups
    .map((group): MessageGroupView => {
      let rows = group.rows;
      if (destructiveActions.has(action)) {
        rows = rows.filter((row) => !ids.has(row.id));
      } else {
        rows = rows.map((row) => {
          if (!ids.has(row.id)) return row;
          if (action === "star" || action === "unstar")
            return { ...row, starred: action === "star" };
          if (action === "read" || action === "unread")
            return { ...row, unread: action === "unread" };
          return row;
        });
      }
      return { ...group, rows };
    })
    .filter((group) => group.rows.length > 0);
  return { ...data, mailbox: { ...data.mailbox, groups } };
}

function snapshotAndMutate(
  qc: QueryClient,
  ids: string[],
  action: MailAction,
): MutationContext {
  const idSet = new Set(ids);
  const snapshots: MutationContext["snapshots"] = [];
  for (const [queryKey, data] of qc.getQueriesData({ queryKey: ["mailbox"] })) {
    if (!isMailboxResponse(data)) continue;
    snapshots.push([queryKey, data]);
    qc.setQueryData(queryKey, mapMailboxRows(data, idSet, action));
  }
  return { snapshots };
}

function restore(qc: QueryClient, context?: MutationContext) {
  for (const [queryKey, data] of context?.snapshots ?? []) {
    qc.setQueryData(queryKey, data);
  }
}

function runAction(
  action: MailAction,
  ids: string[],
  payload?: MailActionPayload,
): Promise<MutationResponse> {
  switch (action) {
    case "archive":
      return archiveMessages(ids);
    case "trash":
      return trashMessages(ids);
    case "spam":
      return spamMessages(ids);
    case "star":
      return starMessages(ids, true);
    case "unstar":
      return starMessages(ids, false);
    case "read":
      return markReadMessages(ids, true);
    case "unread":
      return markReadMessages(ids, false);
    case "read-and-archive":
      return readAndArchiveMessages(ids);
    case "move": {
      if (!payload?.label) throw new Error("move requires a target label");
      return moveMessagesToLabel(ids, payload.label);
    }
    case "label-add": {
      if (!payload?.label) throw new Error("label-add requires a label");
      return modifyLabels(ids, [payload.label], []);
    }
    case "label-remove": {
      if (!payload?.label) throw new Error("label-remove requires a label");
      return modifyLabels(ids, [], [payload.label]);
    }
  }
}

function actionLabel(action: MailAction, payload?: MailActionPayload): string {
  switch (action) {
    case "archive":
      return "Archived";
    case "trash":
      return "Moved to trash";
    case "spam":
      return "Marked spam";
    case "star":
      return "Starred";
    case "unstar":
      return "Unstarred";
    case "read":
      return "Marked read";
    case "unread":
      return "Marked unread";
    case "read-and-archive":
      return "Marked read and archived";
    case "move":
      return payload?.label ? `Moved to ${payload.label}` : "Moved";
    case "label-add":
      return payload?.label ? `Labelled ${payload.label}` : "Labelled";
    case "label-remove":
      return payload?.label ? `Unlabelled ${payload.label}` : "Unlabelled";
  }
}

export interface MailMutationOptions {
  silentSuccess?: boolean;
  /** Required for label-add / label-remove / move. */
  payload?: MailActionPayload;
}

export function useOptimisticMailMutation(action: MailAction, options: MailMutationOptions = {}) {
  const qc = useQueryClient();
  const clearSelection = useSelection((state) => state.clear);
  const { silentSuccess, payload } = options;
  return useMutation({
    mutationFn: (messageIds: string[]) =>
      requestCoordinator.enqueueMutation(() => runAction(action, messageIds, payload)),
    onMutate: async (messageIds) => {
      await qc.cancelQueries({ queryKey: ["mailbox"] });
      const context = snapshotAndMutate(qc, messageIds, action);
      clearSelection();
      return context;
    },
    onError: (error, _messageIds, context) => {
      restore(qc, context);
      toast.error(`${actionLabel(action, payload)} failed`, { description: error.message });
    },
    onSuccess: (response, messageIds) => {
      if (silentSuccess) return;
      const count = response.result?.succeeded ?? messageIds.length;
      const label = actionLabel(action, payload);
      const mutationId = response.result?.mutation_id;
      if (mutationId) {
        toast.success(`${label} ${count}`, {
          duration: 60_000,
          action: {
            label: "Undo",
            onClick: () => {
              undoMutation(mutationId)
                .then(() => {
                  toast.success("Undo applied");
                  void qc.invalidateQueries({ queryKey: ["mailbox"] });
                  void qc.invalidateQueries({ queryKey: ["thread"] });
                  void qc.invalidateQueries({ queryKey: shellKey });
                })
                .catch((error: Error) =>
                  toast.error("Undo failed", { description: error.message }),
                );
            },
          },
        });
      } else {
        toast.success(`${label} ${count}`);
      }
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["mailbox"] });
      void qc.invalidateQueries({ queryKey: ["thread"] });
      void qc.invalidateQueries({ queryKey: shellKey });
    },
  });
}
