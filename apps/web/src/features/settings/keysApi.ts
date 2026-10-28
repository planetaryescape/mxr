/*
 * keys.toml through the daemon: an action's keys or the single-key switch,
 * previewed with a dry run before it is written.
 */

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type KeysChange = components["schemas"]["KeysChangeData"];
export type KeysUpdate = components["schemas"]["KeysUpdateData"];

/** Make `change`, or with `dryRun` only check it. */
export async function updateKeys(change: KeysChange, dryRun: boolean): Promise<KeysUpdate> {
  const response = await apiFetch<{ update: KeysUpdate }>("/api/v1/platform/keymap", {
    method: "POST",
    body: { change, dry_run: dryRun },
  });
  return response.update;
}
