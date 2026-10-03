/*
 * How a mode explains itself (D118): the daemon's one copy table, served
 * by `GetModeGuide`, plus whether the mode's first-encounter card has been
 * retired on this profile. The web, the TUI and the CLI read the same
 * words, and closing the card here closes it in the TUI too.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { getActiveQueryClient } from "@/lib/queryClient";

type Schemas = components["schemas"];
export type ModeGuide = Schemas["ModeGuideData"];
export type ModeKey = Schemas["ModeKeyData"];
type ModeGuides = Extract<Schemas["ResponseData"], { kind: "ModeGuides" }>;

/** Mode ids with a guide: Now and the modes that have shipped. */
export type ModeId = "now" | "todo";

export const modeGuideKey = (mode: ModeId) => ["mode-guide", mode] as const;

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
    // The copy only changes with a new daemon; the seen state changes
    // through `useRetireCard`, which writes the cache itself.
    staleTime: 5 * 60_000,
  });
}

async function postCardSeen(mode: ModeId): Promise<ModeGuides> {
  return apiFetch<ModeGuides>(`/api/v1/mail/modes/${encodeURIComponent(mode)}/card`, {
    method: "POST",
    body: { seen: true },
  });
}

/**
 * Retire a card from outside a component, when the mode's main verb ran
 * (done here retires Now's). Does nothing when the card is already gone or
 * its guide hasn't loaded, so it never costs a request per verb.
 */
export function retireModeCard(mode: ModeId): void {
  const qc = getActiveQueryClient();
  const guide = qc?.getQueryData<ModeGuide>(modeGuideKey(mode));
  if (!qc || !guide || guide.card_seen) return;
  qc.setQueryData(modeGuideKey(mode), { ...guide, card_seen: true });
  postCardSeen(mode).catch(() => {
    qc.setQueryData(modeGuideKey(mode), guide);
  });
}

/**
 * Retire a mode's card in every client. The card leaves at once; a failed
 * write puts it back, since showing a closed card again is the safer miss.
 */
export function useRetireCard(mode: ModeId) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => postCardSeen(mode),
    onMutate: () => {
      const previous = qc.getQueryData<ModeGuide>(modeGuideKey(mode));
      if (previous) qc.setQueryData(modeGuideKey(mode), { ...previous, card_seen: true });
      return { previous };
    },
    onError: (_error, _vars, context) => {
      if (context?.previous) qc.setQueryData(modeGuideKey(mode), context.previous);
    },
    onSuccess: (answer) => {
      const guide = answer.guides.find((entry) => entry.mode === mode);
      if (guide) qc.setQueryData(modeGuideKey(mode), guide);
    },
  });
}
