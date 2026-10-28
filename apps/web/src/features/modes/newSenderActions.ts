/*
 * A new sender's old digit answers (Now and Messages). Each action keeps its
 * id with no key, so pressing 1 to 4 says the answers moved to the `X`
 * picker (`[[action.moved]]` in crates/keymap/keymap.toml) until 0.6.197.
 * The page answers through `X` and `P` now, so no command is registered.
 */

import type { ActionBehavior, ActionScope } from "@/lib/actions/types";

export function newSenderActions(
  scope: Extract<ActionScope, "now" | "messages">,
): ActionBehavior[] {
  return [1, 2, 3, 4].map((slot) => ({
    id: `${scope}.sender-${slot}`,
    command: `senderChoice${slot}`,
  }));
}
