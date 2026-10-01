/*
 * Past-tense wording for a mail action, used in progress and result toasts.
 * The words come from the verb table; only the ones that name a label are
 * built here.
 */

import type { MailAction, MailActionPayload } from "./pendingMailOps";
import { VERB_FEEDBACK } from "./verbFeedback";

export function verb(action: MailAction, payload?: MailActionPayload): string {
  const label = payload?.label;
  switch (action) {
    case "move":
      return label ? `Moved to ${label}` : VERB_FEEDBACK.move.pastTense;
    case "route":
      return label ? `Routed to ${label}` : VERB_FEEDBACK.route.pastTense;
    case "label-add":
      return [VERB_FEEDBACK["label-add"].pastTense, label].filter(Boolean).join(" ");
    case "label-remove":
      return `${VERB_FEEDBACK["label-remove"].pastTense} ${label ?? "a label"} from`;
    case "labels":
      return labelChangeVerb(payload);
    default:
      return VERB_FEEDBACK[action].pastTense;
  }
}

function labelChangeVerb(payload?: MailActionPayload): string {
  const add = payload?.add ?? [];
  const remove = payload?.remove ?? [];
  if (add.length === 1 && remove.length === 0) return verb("label-add", { label: add[0] });
  if (remove.length === 1 && add.length === 0) return verb("label-remove", { label: remove[0] });
  return VERB_FEEDBACK.labels.pastTense;
}
