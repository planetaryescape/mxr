import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

/** The daemon's chime setting, shared with the TUI and CLI. */
export type ChimeSettings = components["schemas"]["NotificationChimesData"];
export type ChimeSoundName = components["schemas"]["NotificationChimeSoundData"];

interface ChimesResponse {
  kind: "NotificationChimes";
  config: ChimeSettings;
}

const CHIMES_PATH = "/api/v1/platform/notifications/chimes";

export async function fetchChimeSettings(): Promise<ChimeSettings> {
  return (await apiFetch<ChimesResponse>(CHIMES_PATH)).config;
}

/** The daemon takes the whole setting; send it back with the change applied. */
export async function saveChimeSettings(config: ChimeSettings): Promise<ChimeSettings> {
  return (await apiFetch<ChimesResponse>(CHIMES_PATH, { method: "POST", body: config })).config;
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
