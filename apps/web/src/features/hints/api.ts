/*
 * Hint seen state (D118, amended 2026-10-07). The copy and the seen flags
 * ride on each mode's guide (`GetModeGuide`); dismissing posts
 * `SetHintSeen`, which the daemon keeps per profile so the TUI and every
 * other client stop showing it too.
 */

import type { QueryClient } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { MODE_GUIDE_ROOT, type ModeGuide } from "@/features/modes/api";
import { getActiveQueryClient } from "@/lib/queryClient";

type Schemas = components["schemas"];
export type Hint = Schemas["HintData"];
type ModeGuides = Extract<Schemas["ResponseData"], { kind: "ModeGuides" }>;

function cachedGuides(qc: QueryClient): [readonly unknown[], ModeGuide][] {
  return qc
    .getQueriesData<ModeGuide>({ queryKey: MODE_GUIDE_ROOT })
    .filter((entry): entry is [readonly unknown[], ModeGuide] => entry[1] !== undefined);
}

/** A hint from whichever cached guide carries it. */
export function cachedHint(id: string): Hint | undefined {
  const qc = getActiveQueryClient();
  if (!qc) return undefined;
  for (const [, guide] of cachedGuides(qc)) {
    const hint = guide.hints.find((entry) => entry.id === id);
    if (hint) return hint;
  }
  return undefined;
}

/** Mark a hint seen (or not) in every cached guide that carries it. */
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

/**
 * Dismiss a hint in every client. The cache flips first so it leaves at
 * once; a failed write flips it back, and the hint shows again at its next
 * need, which is the safer miss.
 */
export async function postHintSeen(id: string): Promise<void> {
  const qc = getActiveQueryClient();
  if (qc) writeSeen(qc, id, true);
  try {
    const answer = await apiFetch<ModeGuides>(`/api/v1/mail/hints/${encodeURIComponent(id)}`, {
      method: "POST",
      body: { seen: true },
    });
    if (!qc) return;
    // Only guides this client holds: the daemon may answer for a mode the
    // web app has no page for yet.
    for (const [key, cached] of cachedGuides(qc)) {
      const fresh = answer.guides.find((guide) => guide.mode === cached.mode);
      if (fresh) qc.setQueryData(key, fresh);
    }
  } catch {
    if (qc) writeSeen(qc, id, false);
  }
}
