import type { ActionBehavior, ActionScope } from "@/lib/actions/types";

export function newSenderActions(
  scope: Extract<ActionScope, "now" | "messages">,
): ActionBehavior[] {
  return [1, 2, 3, 4].map((slot) => ({
    id: `${scope}.sender-${slot}`,
    command: `senderChoice${slot}`,
  }));
}
