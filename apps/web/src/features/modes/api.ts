import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

type Schemas = components["schemas"];
export type ModeGuide = Schemas["ModeGuideData"];
export type ModeKey = Schemas["ModeKeyData"];
type ModeGuides = Extract<Schemas["ResponseData"], { kind: "ModeGuides" }>;

export type ModeId = "now" | "messages" | "todo" | "updates" | "reading" | "archive";

export const MODE_GUIDE_ROOT = ["mode-guide"] as const;
export const modeGuideKey = (mode: ModeId) => [...MODE_GUIDE_ROOT, mode] as const;

export async function fetchModeGuide(mode: ModeId): Promise<ModeGuide> {
  const answer = await apiFetch<ModeGuides>(
    `/api/v1/mail/modes/guide?mode=${encodeURIComponent(mode)}`,
  );
  const guide = answer.guides.find((entry) => entry.mode === mode);
  if (!guide) throw new Error(`The daemon has no guide for ${mode}`);
  return guide;
}

export function useModeGuide(mode: ModeId) {
  return useQuery({
    queryKey: modeGuideKey(mode),
    queryFn: () => fetchModeGuide(mode),
    staleTime: 5 * 60_000,
  });
}
