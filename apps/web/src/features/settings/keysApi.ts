import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type KeysChange = components["schemas"]["KeysChangeData"];
export type KeysUpdate = components["schemas"]["KeysUpdateData"];

export async function updateKeys(change: KeysChange, dryRun: boolean): Promise<KeysUpdate> {
  const response = await apiFetch<{ update: KeysUpdate }>("/api/v1/platform/keymap", {
    method: "POST",
    body: { change, dry_run: dryRun },
  });
  return response.update;
}
