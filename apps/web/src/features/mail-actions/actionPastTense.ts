/* Past-tense wording for a mail action, used in progress and result toasts. */

import type { MailAction, MailActionPayload } from "./pendingMailOps";

export function verb(action: MailAction, payload?: MailActionPayload): string {
  switch (action) {
    case "archive":
      return "Archived";
    case "trash":
      return "Moved to Trash";
    case "spam":
      return "Marked as spam";
    case "star":
      return "Starred";
    case "unstar":
      return "Unstarred";
    case "read":
      return "Marked read";
    case "unread":
      return "Marked unread";
    case "read-and-archive":
      return "Read and archived";
    case "move":
      return payload?.label ? `Moved to ${payload.label}` : "Moved";
    case "route":
      return payload?.label ? `Routed to ${payload.label}` : "Routed";
    case "label-add":
      return payload?.label ? `Labelled ${payload.label}` : "Labelled";
    case "label-remove":
      return payload?.label ? `Removed ${payload.label}` : "Label removed";
    case "labels":
      return labelChangeVerb(payload);
    case "snooze":
      return "Snoozed";
  }
}

function labelChangeVerb(payload?: MailActionPayload): string {
  const add = payload?.add ?? [];
  const remove = payload?.remove ?? [];
  if (add.length === 1 && remove.length === 0) return `Labelled ${add[0]}`;
  if (remove.length === 1 && add.length === 0) return `Removed ${remove[0]} from`;
  return "Updated labels on";
}
