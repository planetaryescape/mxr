import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { usePanelRef } from "react-resizable-panels";

import { panesWidth, readPaneWidth, writePaneWidth } from "@/hooks/paneWidth";

/** A split's side pane: the list beside a reader, or Archive's card. */
export interface SidePaneSize {
  /** px, rem or %, as the panel library takes them. */
  defaultSize: string;
  minSize: string;
  maxSize: string;
}

/** The other pane (the reader, the ledger) is never squeezed below this. */
const OTHER_PANE_MIN = "28rem";

/** The handle's own resize keys (the library's), as opposed to Tab. */
const RESIZE_KEYS = new Set(["ArrowLeft", "ArrowRight", "Home", "End", "Enter"]);

/** The longest an opened split waits for idle time before fitting. */
const IDLE_FIT_TIMEOUT_MS = 500;

/** How long a key press or drag end stays "the person's" without a change. */
const MARK_LAPSE_MS = 250;

/*
 * The side pane's width as a saved preference in pixels, fitted to the
 * room there is now. The library keeps a pane's pixels as its group
 * resizes, so on its own a 640px list saved on a wide screen stays 640px
 * in a 768px window and the reader shrinks to nothing. Here the other pane
 * (the reader, the ledger) has a floor the library enforces, the side
 * pane's own minimum gives way when both can't fit, the pane is fitted on
 * load and on every resize, and only the person's resizing (a drag, the
 * handle's keys, a double-click) changes the preference. When the room
 * comes back, so does the width. `active` is false while the panes don't
 * sit side by side.
 */
export function useSplitPane(id: string, size: SidePaneSize, { active }: { active: boolean }) {
  const key = `mxr:split:${id}`;
  const panelRef = usePanelRef();
  const groupRef = useRef<HTMLDivElement | null>(null);
  const [saved] = useState(() => readPaneWidth(key));
  const preference = useRef<number | null>(saved);
  // The side pane's minimum in px once the window is too small for it and
  // the other pane's floor together; null while the static one fits.
  const [squeezedMin, setSqueezedMin] = useState<number | null>(null);
  // The group's width when the person started resizing, or null when the
  // current change isn't theirs (the window, the sidebar, this hook).
  const resizing = useRef<number | null>(null);

  const groupWidth = () => groupRef.current?.getBoundingClientRect().width ?? 0;

  const fit = useCallback(() => {
    const panel = panelRef.current;
    const width = groupRef.current?.getBoundingClientRect().width ?? 0;
    if (!active || width === 0) return;
    const room = Math.max(0, width - toPx(OTHER_PANE_MIN, width) - 1);
    const max = Math.min(toPx(size.maxSize, width), room);
    const min = Math.min(toPx(size.minSize, width), room);
    setSqueezedMin(min < toPx(size.minSize, width) ? min : null);
    if (!panel) return;
    const wanted = preference.current ?? toPx(size.defaultSize, width);
    const target = Math.min(max, Math.max(min, wanted));
    if (Math.abs(panel.getSize().inPixels - target) > 0.5) {
      resizing.current = null;
      panel.resize(`${target}px`);
    }
  }, [active, panelRef, size.defaultSize, size.maxSize, size.minSize]);

  // Before the first paint when the split mounts side by side, so a saved
  // width never jumps. When a conversation opens into a split that was a
  // lone list, once the browser is idle: opening is on the speed gate's
  // path, and fitting reads layout, which costs a forced layout in the
  // frames that paint the conversation. The layout from the last open
  // still applies, so only a window resized while the list was alone has
  // anything to fix. After that, a frame after each change of the group's
  // width: the library hears of it from its own observer, and a resize
  // asked for before then is sized against the old width.
  const mounted = useRef(false);
  useLayoutEffect(() => {
    const firstRun = !mounted.current;
    mounted.current = true;
    const group = groupRef.current;
    if (!group || !active) return;
    if (firstRun) fit();
    const idle = whenIdle(fit);
    let frame = 0;
    let initial = true;
    const observer = new ResizeObserver(() => {
      // Observing reports the current size once: the idle fit has that.
      if (initial) {
        initial = false;
        return;
      }
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(fit);
    });
    observer.observe(group);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
      idle.cancel();
    };
  }, [active, fit]);
  // A squeezed minimum re-registers the panel, which the library applies a
  // render later: fit again on the next frame. Keyed on the minimum alone:
  // `fit` changes whenever the split opens, and an extra fit then would read
  // layout in the frame that paints the conversation.
  const latestFit = useRef(fit);
  useLayoutEffect(() => {
    latestFit.current = fit;
  }, [fit]);
  useLayoutEffect(() => {
    const frame = requestAnimationFrame(() => latestFit.current());
    return () => cancelAnimationFrame(frame);
  }, [squeezedMin]);

  // Marks the next layout change as the person's. The library reports it
  // a render after its own keydown or pointerup listener resizes, so the
  // mark goes on in the capture phase (before those listeners) and lapses
  // shortly after the key or the drag ends if nothing changed.
  const startResizing = (until: "keydown" | "pointerup") => {
    const mark = groupWidth();
    resizing.current = mark;
    const lapse = () =>
      window.setTimeout(() => {
        if (resizing.current === mark) resizing.current = null;
      }, MARK_LAPSE_MS);
    if (until === "keydown") lapse();
    else window.addEventListener("pointerup", lapse, { once: true });
  };
  const onLayoutChange = useCallback(() => {
    // A drag can start in the hit strip just beside the handle's element,
    // so its pointerdown never reaches the handle: catch it mid-drag.
    if (resizing.current === null && groupRef.current?.querySelector('[data-separator="active"]')) {
      resizing.current = groupWidth();
    }
  }, []);
  const onLayoutChanged = useCallback(() => {
    const panel = panelRef.current;
    const startedAt = resizing.current;
    resizing.current = null;
    if (!active || !panel || startedAt === null || startedAt !== groupWidth()) return;
    // The DOM hasn't caught up yet: take the new share of the panes' width.
    preference.current = Math.round(
      (panel.getSize().asPercentage / 100) * panesWidth(groupRef.current),
    );
    writePaneWidth(key, preference.current);
  }, [active, key, panelRef]);

  const onDoubleClick = useCallback(
    (event: MouseEvent) => {
      event.preventDefault();
      preference.current = null;
      writePaneWidth(key, null);
      fit();
    },
    [fit, key],
  );

  return {
    groupProps: { elementRef: groupRef, onLayoutChange, onLayoutChanged },
    sidePanelProps: {
      panelRef,
      // The saved width paints on the first frame; fit() corrects it before
      // paint when the window is too small for it.
      defaultSize: saved === null ? size.defaultSize : `${saved}px`,
      minSize: squeezedMin === null ? size.minSize : `${squeezedMin}px`,
      maxSize: size.maxSize,
      groupResizeBehavior: "preserve-pixel-size" as const,
    },
    otherPanelProps: { minSize: OTHER_PANE_MIN },
    handleProps: {
      // The library resets to `defaultSize`, which here may be the saved
      // width; this resets to the real default and forgets the preference.
      disableDoubleClick: true,
      onDoubleClick,
      onPointerDownCapture: () => startResizing("pointerup"),
      onKeyDownCapture: (event: KeyboardEvent) => {
        if (RESIZE_KEYS.has(event.key)) startResizing("keydown");
      },
    },
  };
}

/** Runs `run` once the browser is idle (soon, where idle callbacks don't exist). */
function whenIdle(run: () => void): { cancel: () => void } {
  if ("requestIdleCallback" in window) {
    const handle = window.requestIdleCallback(run, { timeout: IDLE_FIT_TIMEOUT_MS });
    return { cancel: () => window.cancelIdleCallback(handle) };
  }
  const timer = globalThis.setTimeout(run, 0);
  return { cancel: () => globalThis.clearTimeout(timer) };
}

/** A library size string in pixels, against the group's width. */
function toPx(value: string, groupWidth: number): number {
  const amount = Number.parseFloat(value);
  if (value.endsWith("rem")) {
    return amount * Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  }
  if (value.endsWith("px")) return amount;
  return (amount / 100) * groupWidth;
}
