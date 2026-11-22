import { useCallback, useState } from "react";

export type TrustHintId = "now.not_sure" | "now.move";

const storageKey = (id: TrustHintId) => `mxr:hint-seen:${id}`;

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
  } catch {}
}

export function resetHintsForTests(): void {
  seenThisSession.clear();
}

export interface ShownTrustHint {
  text: string | undefined;
  dismiss: () => void;
}

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
