import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type ChimeSettings = components["schemas"]["NotificationChimesData"];
export type ChimeSoundName = components["schemas"]["NotificationChimeSoundData"];
export type ChimesPatch = Partial<components["schemas"]["NotificationChimesPatchData"]>;

interface ChimesResponse {
  kind: "NotificationChimes";
  config: ChimeSettings;
}

const CHIMES_PATH = "/api/v1/platform/notifications/chimes";

export async function fetchChimeSettings(): Promise<ChimeSettings> {
  return (await apiFetch<ChimesResponse>(CHIMES_PATH)).config;
}

export async function saveChimeSettings(patch: ChimesPatch): Promise<ChimeSettings> {
  return (await apiFetch<ChimesResponse>(CHIMES_PATH, { method: "POST", body: patch })).config;
}

export const chimeSettingsQuery = {
  queryKey: ["notification-chimes"] as const,
  queryFn: fetchChimeSettings,
  staleTime: 5 * 60_000,
  retry: false,
};
