import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

/** The daemon's chime setting, shared with the TUI and CLI. */
export type ChimeSettings = components["schemas"]["NotificationChimesData"];
export type ChimeSoundName = components["schemas"]["NotificationChimeSoundData"];
/**
 * The fields to change; the daemon applies them to the setting as it is
 * then. Partial: the Rust type is `#[serde(default)]`, so an absent field
 * means "keep", though the generated schema lists every field.
 */
export type ChimesPatch = Partial<components["schemas"]["NotificationChimesPatchData"]>;

interface ChimesResponse {
  kind: "NotificationChimes";
  config: ChimeSettings;
}

const CHIMES_PATH = "/api/v1/platform/notifications/chimes";

export async function fetchChimeSettings(): Promise<ChimeSettings> {
  return (await apiFetch<ChimesResponse>(CHIMES_PATH)).config;
}

/** Change some fields; answers with the whole setting as the daemon now has it. */
export async function saveChimeSettings(patch: ChimesPatch): Promise<ChimeSettings> {
  return (await apiFetch<ChimesResponse>(CHIMES_PATH, { method: "POST", body: patch })).config;
}

/**
 * Read once at start-up and on the Sound settings page. The player reads the
 * cache, never subscribes, so a changed setting re-renders nothing else.
 */
export const chimeSettingsQuery = {
  queryKey: ["notification-chimes"] as const,
  queryFn: fetchChimeSettings,
  staleTime: 5 * 60_000,
  retry: false,
};
