import { useRouterState } from "@tanstack/react-router";
import { useCallback, useEffect, useRef } from "react";

import { useModeGuide, type ModeId } from "@/features/modes/api";

import { postHintSeen, type Hint } from "./api";
import { listenForInteraction, pageOf, useHintSlot } from "./hintSlot";

export interface HintNeed {
  ready: boolean;
  alone?: boolean;
}

export interface ShownHint {
  hint: Hint | undefined;
  dismiss: () => void;
}

export function dismissHint(id: string): void {
  useHintSlot.getState().release(id);
  void postHintSeen(id);
}

export function useHint(mode: ModeId, id: string, need: HintNeed): ShownHint {
  const hint = useModeGuide(mode).data?.hints.find((entry) => entry.id === id);
  const ready = need.ready && Boolean(hint && !hint.seen);
  const page = useRouterState({ select: (s) => pageOf(s.location.pathname) });
  const active = useHintSlot((s) => s.active === id);
  const free = useHintSlot((s) => s.active === null);
  const allowed = useHintSlot((s) => s.interactedOn === page) || Boolean(need.alone);
  const epoch = useHintSlot((s) => s.epoch);
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

  useEffect(() => {
    if (active && !ready) useHintSlot.getState().release(id);
  }, [active, id, ready]);
  useEffect(() => () => useHintSlot.getState().release(id), [id]);

  const dismiss = useCallback(() => {
    if (useHintSlot.getState().active === id) dismissHint(id);
  }, [id]);
  return { hint: active && ready ? hint : undefined, dismiss };
}

export function useActiveHintDismiss(): (() => void) | undefined {
  const active = useHintSlot((s) => s.active);
  return active ? () => dismissHint(active) : undefined;
}
