import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  archiveMessages,
  markReadMessages,
  shellKey,
  spamMessages,
  starMessages,
  trashMessages,
  undoMutation,
} from "./api";
import type { MailboxResponse, MessageGroupView, MutationResponse } from "./types";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { useSelection } from "@/state/selectionStore";

export type MailAction = "archive" | "trash" | "spam" | "star" | "unstar" | "read" | "unread";

interface MutationContext {
  snapshots: Array<[readonly unknown[], unknown]>;
}

const destructiveActions = new Set<MailAction>(["archive", "trash", "spam"]);

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

function snapshotAndMutate(qc: QueryClient, ids: string[], action: MailAction): MutationContext {
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

function runAction(action: MailAction, ids: string[]): Promise<MutationResponse> {
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
  }
}

function actionLabel(action: MailAction): string {
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
  }
}

export function useOptimisticMailMutation(
  action: MailAction,
  options: { silentSuccess?: boolean } = {},
) {
  const qc = useQueryClient();
  const clearSelection = useSelection((state) => state.clear);
  return useMutation({
    mutationFn: (messageIds: string[]) =>
      requestCoordinator.enqueueMutation(() => runAction(action, messageIds)),
    onMutate: async (messageIds) => {
      await qc.cancelQueries({ queryKey: ["mailbox"] });
      const context = snapshotAndMutate(qc, messageIds, action);
      clearSelection();
      return context;
    },
    onError: (error, _messageIds, context) => {
      restore(qc, context);
      toast.error(`${actionLabel(action)} failed`, { description: error.message });
    },
    onSuccess: (response, messageIds) => {
      if (options.silentSuccess) return;
      const count = response.result?.succeeded ?? messageIds.length;
      const mutationId = response.result?.mutation_id;
      if (mutationId) {
        toast.success(`${actionLabel(action)} ${count}`, {
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
        toast.success(`${actionLabel(action)} ${count}`);
      }
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["mailbox"] });
      void qc.invalidateQueries({ queryKey: ["thread"] });
      void qc.invalidateQueries({ queryKey: shellKey });
    },
  });
}
