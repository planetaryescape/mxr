/* The daemon request behind each mail action, for runs below the job threshold. */

import {
  archiveMessages,
  markReadMessages,
  modifyLabels,
  moveMessagesToLabel,
  readAndArchiveMessages,
  routeMessages,
  snoozeMessage,
  spamMessages,
  starMessages,
  trashMessages,
} from "@/features/mailbox/api";
import type { MutationResponse } from "@/features/mailbox/types";
import type { MailMutationOptions } from "./mailMutations";
import type { MailAction } from "./pendingMailOps";

export async function runAction(
  action: MailAction,
  ids: string[],
  options: MailMutationOptions,
): Promise<MutationResponse> {
  const payload = options.payload;
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
    case "move":
      if (!payload?.label) throw new Error("Choose a label to move to");
      return moveMessagesToLabel(ids, payload.label);
    case "route":
      if (!payload?.label) throw new Error("Choose a label to route to");
      if (!payload.fromQueueLabel) throw new Error("Open a queue label before routing");
      return routeMessages({
        messageIds: ids,
        toLabel: payload.label,
        fromQueueLabel: payload.fromQueueLabel,
        archive: payload.archive ?? true,
      });
    case "label-add":
      if (!payload?.label) throw new Error("Choose a label");
      return modifyLabels(ids, [payload.label], []);
    case "label-remove":
      if (!payload?.label) throw new Error("Choose a label");
      return modifyLabels(ids, [], [payload.label]);
    case "labels":
      if (!payload?.add?.length && !payload?.remove?.length) throw new Error("No label changes");
      return modifyLabels(ids, payload.add ?? [], payload.remove ?? []);
    case "snooze":
      return snoozeAll(ids, options.until);
  }
}

/**
 * The bridge snoozes one message per request. Report partial failure in the
 * same shape as batch mutations so the caller's accounting stays honest.
 */
async function snoozeAll(ids: string[], until?: string): Promise<MutationResponse> {
  if (!until) throw new Error("Choose when to snooze until");
  const results = await Promise.allSettled(
    ids.map((messageId) => snoozeMessage({ messageId, until })),
  );
  const failed = results.filter((result) => result.status === "rejected");
  const firstError = failed[0]?.status === "rejected" ? failed[0].reason : null;
  return {
    ok: failed.length === 0,
    result: {
      requested: ids.length,
      succeeded: ids.length - failed.length,
      skipped: 0,
      failed: failed.length,
      accounts: firstError
        ? [
            {
              account_id: "",
              account_name: "snooze",
              succeeded: ids.length - failed.length,
              skipped: 0,
              failed: failed.length,
              error: firstError instanceof Error ? firstError.message : String(firstError),
            },
          ]
        : undefined,
    },
  };
}
