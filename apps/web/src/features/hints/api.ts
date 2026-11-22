import type { QueryClient } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { MODE_GUIDE_ROOT, type ModeGuide } from "@/features/modes/api";
import { getActiveQueryClient } from "@/lib/queryClient";

type Schemas = components["schemas"];
export type Hint = Schemas["HintData"];
type ModeGuides = Extract<Schemas["ResponseData"], { kind: "ModeGuides" }>;
type HintSeen = Extract<Schemas["ResponseData"], { kind: "HintSeen" }>;

export async function postNextTimeSeen(actionId: string): Promise<boolean> {
  const answer = await apiFetch<HintSeen>(
    `/api/v1/mail/hints/${encodeURIComponent(`next-time.${actionId}`)}`,
    { method: "POST", body: { seen: true } },
  );
  return answer.newly_seen;
}

function cachedGuides(qc: QueryClient): [readonly unknown[], ModeGuide][] {
  return qc
    .getQueriesData<ModeGuide>({ queryKey: MODE_GUIDE_ROOT })
    .filter((entry): entry is [readonly unknown[], ModeGuide] => entry[1] !== undefined);
}

export function cachedHint(id: string): Hint | undefined {
  const qc = getActiveQueryClient();
  if (!qc) return undefined;
  for (const [, guide] of cachedGuides(qc)) {
    const hint = guide.hints.find((entry) => entry.id === id);
    if (hint) return hint;
  }
  return undefined;
}

function writeSeen(qc: QueryClient, id: string, seen: boolean): void {
  for (const [key, guide] of cachedGuides(qc)) {
    const at = guide.hints.findIndex((hint) => hint.id === id);
    const hint = guide.hints[at];
    if (!hint) continue;
    const hints = guide.hints.slice();
    hints[at] = { ...hint, seen };
    qc.setQueryData<ModeGuide>(key, { ...guide, hints });
  }
}

export async function postHintSeen(id: string): Promise<void> {
  const qc = getActiveQueryClient();
  if (qc) writeSeen(qc, id, true);
  try {
    const answer = await apiFetch<ModeGuides>(`/api/v1/mail/hints/${encodeURIComponent(id)}`, {
      method: "POST",
      body: { seen: true },
    });
    if (!qc) return;
    for (const [key, cached] of cachedGuides(qc)) {
      const fresh = answer.guides.find((guide) => guide.mode === cached.mode);
      if (fresh) qc.setQueryData(key, fresh);
    }
  } catch {
    if (qc) writeSeen(qc, id, false);
  }
}
