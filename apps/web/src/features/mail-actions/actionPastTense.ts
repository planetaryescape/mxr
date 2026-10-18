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

function failurePhrase(
  action: MailAction,
  payload: MailActionPayload | undefined,
  form: "base" | "partial",
): string {
  const label = payload?.label;
  const template = VERB_FEEDBACK[action].failure?.[form] ?? `${verb(action, payload)} {}`;
  if (action === "move" || action === "route") return label ? `${template} to ${label}` : template;
  if (action === "label-add" && label) {
    return form === "base" ? `add ${label} to {}` : `Added ${label} to {}`;
  }
  if (action === "label-remove" && label) {
    return form === "base" ? `remove ${label} from {}` : `Removed ${label} from {}`;
  }
  if (action === "labels") {
    const add = payload?.add ?? [];
    const remove = payload?.remove ?? [];
    if (add.length === 1 && remove.length === 0)
      return failurePhrase("label-add", { label: add[0] }, form);
    if (remove.length === 1 && add.length === 0) {
      return failurePhrase("label-remove", { label: remove[0] }, form);
    }
  }
  return template;
}

/** Nothing changed: "Couldn't archive 3 messages". */
export function failureTitle(
  action: MailAction,
  payload: MailActionPayload | undefined,
  object: string,
): string {
  return `Couldn't ${failurePhrase(action, payload, "base").replace("{}", object)}`;
}

/** Part changed: "Archived 2 of 3 messages; the rest failed". */
export function partialFailureTitle(
  action: MailAction,
  payload: MailActionPayload | undefined,
  object: string,
): string {
  return `${failurePhrase(action, payload, "partial").replace("{}", object)}; the rest failed`;
}
