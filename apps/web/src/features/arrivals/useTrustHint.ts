/*
 * A stand-in for the contextual hints of fix/contextual-hints
 * (`features/hints/useHint.ts`), with the same shape: one hint per id,
 * shown at its element the first time the element is needed, gone once
 * dismissed. When that branch lands, its daemon guide carries these two
 * hints ("now.not_sure", "now.move") and this file becomes a call to
 * `useHint("now", id, need)`. Until then the seen state is this browser's
 * only, which is a convenience, not a record anyone else reads.
 */

import { useCallback, useState } from "react";

export type TrustHintId = "now.not_sure" | "now.move";

const storageKey = (id: TrustHintId) => `mxr:hint-seen:${id}`;

/** Seen this session, so blocked storage still shows a hint once per visit. */
const seenThisSession = new Set<TrustHintId>();

export function hintSeen(id: TrustHintId): boolean {
  if (seenThisSession.has(id)) return true;
  try {
    return window.localStorage.getItem(storageKey(id)) !== null;
  } catch {
    return false;
  }
}

export function markHintSeen(id: TrustHintId): void {
  seenThisSession.add(id);
  try {
    window.localStorage.setItem(storageKey(id), new Date().toISOString());
  } catch {
    // Private windows and blocked storage: the hint shows again next visit.
  }
}

/** Test-only: forget what this session has seen. */
export function resetHintsForTests(): void {
  seenThisSession.clear();
}

export interface ShownTrustHint {
  /** Set while the hint should show at its element. */
  text: string | undefined;
  dismiss: () => void;
}

/** One hint: `text` comes from the daemon, `ready` says the element is on screen. */
export function useTrustHint(id: TrustHintId, text: string | null | undefined, ready: boolean) {
  const [seen, setSeen] = useState(() => hintSeen(id));
  const dismiss = useCallback(() => {
    markHintSeen(id);
    setSeen(true);
  }, [id]);
  const shown: ShownTrustHint = {
    text: ready && !seen && text ? text : undefined,
    dismiss,
  };
  return shown;
}
