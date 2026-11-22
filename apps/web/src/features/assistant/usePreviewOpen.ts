import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

import { pressedOverlayKey } from "@/lib/keys/overlayKeys";

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
  const pointerFocus = useRef(false);
  const openTimer = useRef<Timer>(null);
  const closeTimer = useRef<Timer>(null);

  const escape = useRef<((event: globalThis.KeyboardEvent) => void) | null>(null);
  const pressEnd = useRef<(() => void) | null>(null);

  const syncEscape = () => {
    const wanted = openTimer.current !== null || isOpen.current;
    if (wanted && !escape.current) {
      escape.current = (event) => {
        if (pressedOverlayKey(event, "popover.close")) suppress();
      };
      document.addEventListener("keydown", escape.current, true);
    } else if (!wanted && escape.current) {
      document.removeEventListener("keydown", escape.current, true);
      escape.current = null;
    }
  };
  const endPress = () => {
    pointerFocus.current = false;
    if (pressEnd.current) {
      document.removeEventListener("pointerup", pressEnd.current);
      document.removeEventListener("pointercancel", pressEnd.current);
      pressEnd.current = null;
    }
  };

  const setOpen = (next: boolean) => {
    isOpen.current = next;
    setOpenState(next);
    syncEscape();
  };
  const clearOpen = () => {
    if (openTimer.current) clearTimeout(openTimer.current);
    openTimer.current = null;
    syncEscape();
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
      endPress();
      if (escape.current) {
        document.removeEventListener("keydown", escape.current, true);
        escape.current = null;
      }
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
      else syncEscape();
    }, OPEN_DELAY_MS);
    syncEscape();
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
    dismiss: shut,
    linkProps: {
      onPointerEnter: (event: PointerEvent) => {
        if (event.pointerType === "touch") return;
        hoveredLink.current = true;
        suppressed.current = false;
        startOpen();
      },
      onPointerLeave: (event: PointerEvent) => {
        endPress();
        if (event.pointerType === "touch") return;
        hoveredLink.current = false;
        settle();
      },
      onPointerDown: () => {
        endPress();
        pointerFocus.current = true;
        pressEnd.current = endPress;
        document.addEventListener("pointerup", endPress);
        document.addEventListener("pointercancel", endPress);
        suppress();
      },
      onClick: () => {
        endPress();
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
        endPress();
        focused.current = false;
        settle();
      },
      onKeyDown: (event: KeyboardEvent) => {
        if (event.key === "Enter" || event.key === " ") suppress();
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
        event.preventDefault();
        suppress();
      },
    },
  };
}
