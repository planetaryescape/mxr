import { useRouterState } from "@tanstack/react-router";
import { useCallback, useEffect, useRef } from "react";

import { useModeGuide, type ModeId } from "@/features/modes/api";

import { postHintSeen, type Hint } from "./api";
import { listenForInteraction, pageOf, useHintSlot } from "./hintSlot";

export interface HintNeed {
  /** The element is on screen and at its moment of need. */
  ready: boolean;
  /** The element is the only thing on the page, so it may show on arrival. */
  alone?: boolean;
}

export interface ShownHint {
  /** Set while this hint holds the slot: render it at its element. */
  hint: Hint | undefined;
  /** Esc, its close button, or acting on the element. A no-op when hidden. */
  dismiss: () => void;
}

/** Dismiss a hint in every client and free the slot. */
export function dismissHint(id: string): void {
  useHintSlot.getState().release(id);
  void postHintSeen(id);
}

/**
 * One hint, anchored by the caller to one element. Call it once per hint
 * id, where the element's readiness is known, and render the returned hint
 * at that element.
 */
export function useHint(mode: ModeId, id: string, need: HintNeed): ShownHint {
  const hint = useModeGuide(mode).data?.hints.find((entry) => entry.id === id);
  const ready = need.ready && Boolean(hint && !hint.seen);
  const page = useRouterState({ select: (s) => pageOf(s.location.pathname) });
  const active = useHintSlot((s) => s.active === id);
  const free = useHintSlot((s) => s.active === null);
  const allowed = useHintSlot((s) => s.interactedOn === page) || Boolean(need.alone);
  const epoch = useHintSlot((s) => s.epoch);
  // The epoch when the element last became needed.
  const neededAt = useRef<number | null>(null);

  useEffect(listenForInteraction, []);

  useEffect(() => {
    if (!ready) {
      neededAt.current = null;
      return;
    }
    neededAt.current ??= epoch;
    if (free && allowed && neededAt.current === epoch) useHintSlot.getState().claim(id);
  }, [allowed, epoch, free, id, ready]);

  // The element went away, or the hint was dismissed in another client.
  useEffect(() => {
    if (active && !ready) useHintSlot.getState().release(id);
  }, [active, id, ready]);
  useEffect(() => () => useHintSlot.getState().release(id), [id]);

  const dismiss = useCallback(() => {
    if (useHintSlot.getState().active === id) dismissHint(id);
  }, [id]);
  return { hint: active && ready ? hint : undefined, dismiss };
}

/** Esc's job while a hint shows: dismiss it. Undefined when none is up. */
export function useActiveHintDismiss(): (() => void) | undefined {
  const active = useHintSlot((s) => s.active);
  return active ? () => dismissHint(active) : undefined;
}
