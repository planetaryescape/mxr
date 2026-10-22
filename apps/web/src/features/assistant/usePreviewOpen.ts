/*
 * When a link's preview card is open. One small state machine we own, in
 * place of a hover library's timers: those run separately for hover and for
 * focus, can't be cancelled together, and skip closing during a press.
 *
 * State (refs; only `open` is React state):
 *   hoveredLink, hoveredCard  a mouse or pen is over the link / the card
 *   focused                   the link has keyboard focus (focus that a press
 *                             caused doesn't count: it is part of the click)
 *   suppressed                the link was activated or Esc was pressed, so the
 *                             current hover or focus must not open the card
 *   openTimer, closeTimer     at most one each
 *
 * Events and what they do:
 *   mouse/pen pointerenter, link  hoveredLink; suppression ends (a fresh
 *                                 hover); if closed, (re)start openTimer
 *   keyboard focus, link          focused; suppression ends; if closed, start
 *                                 openTimer
 *   re-entry (link, card, focus)  clears closeTimer
 *   openTimer fires               open only if (hoveredLink or focused) and
 *                                 not suppressed
 *   leave link/card, blur         if nothing is hovered or focused: clear
 *                                 openTimer, start closeTimer(200)
 *   closeTimer fires              close if still nothing hovered or focused
 *   activation: pointerdown (any pointer type), Enter or Space keydown, click
 *                                 clear both timers, close, suppressed
 *   Esc (card or link)            clear both timers, close, suppressed
 *   any touch pointer event       ignored for hover state, so a tap never
 *                                 opens the card
 *   unmount                       clears both timers
 *
 * Suppression blocks only the hover or focus that is current when it is set;
 * the next mouse enter or keyboard focus clears it. It never waits for a
 * pointerup, click or leave to arrive, so a press released elsewhere, a
 * cancelled pointer or a touch tap's odd event order can't leave it stuck.
 */

import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

export const OPEN_DELAY_MS = 400;
export const CLOSE_DELAY_MS = 200;

type Timer = ReturnType<typeof setTimeout> | null;

export function usePreviewOpen() {
  const [open, setOpenState] = useState(false);
  const isOpen = useRef(false);
  const hoveredLink = useRef(false);
  const hoveredCard = useRef(false);
  const focused = useRef(false);
  const suppressed = useRef(false);
  // A press focuses the link; that focus is not keyboard focus.
  const pointerFocus = useRef(false);
  const openTimer = useRef<Timer>(null);
  const closeTimer = useRef<Timer>(null);

  const setOpen = (next: boolean) => {
    isOpen.current = next;
    setOpenState(next);
  };
  const clearOpen = () => {
    if (openTimer.current) clearTimeout(openTimer.current);
    openTimer.current = null;
  };
  const clearClose = () => {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    closeTimer.current = null;
  };
  const attended = () => hoveredLink.current || hoveredCard.current || focused.current;

  useEffect(
    () => () => {
      clearOpen();
      clearClose();
    },
    [],
  );

  const startOpen = () => {
    clearClose();
    if (isOpen.current || suppressed.current) return;
    clearOpen();
    openTimer.current = setTimeout(() => {
      openTimer.current = null;
      if (!suppressed.current && (hoveredLink.current || focused.current)) setOpen(true);
    }, OPEN_DELAY_MS);
  };
  const settle = () => {
    if (attended()) return;
    clearOpen();
    if (!isOpen.current) return;
    clearClose();
    closeTimer.current = setTimeout(() => {
      closeTimer.current = null;
      if (!attended()) setOpen(false);
    }, CLOSE_DELAY_MS);
  };
  const shut = () => {
    hoveredCard.current = false;
    clearOpen();
    clearClose();
    setOpen(false);
  };
  const suppress = () => {
    shut();
    suppressed.current = true;
  };

  return {
    open,
    /** Close without suppressing (an outside press or focus). */
    dismiss: shut,
    linkProps: {
      onPointerEnter: (event: PointerEvent) => {
        if (event.pointerType === "touch") return;
        hoveredLink.current = true;
        suppressed.current = false;
        startOpen();
      },
      onPointerLeave: (event: PointerEvent) => {
        if (event.pointerType === "touch") return;
        hoveredLink.current = false;
        settle();
      },
      onPointerDown: () => {
        pointerFocus.current = true;
        suppress();
      },
      // Best effort: the press's focus arrives straight after pointerdown and
      // consumes the flag; these only clear it when the press caused no focus.
      onPointerUp: () => {
        pointerFocus.current = false;
      },
      onClick: () => {
        pointerFocus.current = false;
        suppress();
      },
      onFocus: () => {
        if (pointerFocus.current) {
          pointerFocus.current = false;
          return;
        }
        focused.current = true;
        suppressed.current = false;
        startOpen();
      },
      onBlur: () => {
        pointerFocus.current = false;
        focused.current = false;
        settle();
      },
      onKeyDown: (event: KeyboardEvent) => {
        if (event.key === "Enter" || event.key === " " || event.key === "Escape") suppress();
      },
    },
    cardProps: {
      onPointerEnter: (event: PointerEvent) => {
        if (event.pointerType === "touch") return;
        hoveredCard.current = true;
        clearClose();
      },
      onPointerLeave: (event: PointerEvent) => {
        if (event.pointerType === "touch") return;
        hoveredCard.current = false;
        settle();
      },
      onEscapeKeyDown: (event: { preventDefault: () => void }) => {
        // Handled here so the narrow-screen drawer, which also closes on Esc,
        // stays open.
        event.preventDefault();
        suppress();
      },
    },
  };
}
