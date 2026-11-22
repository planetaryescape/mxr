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

export interface SidePaneSize {
  defaultSize: string;
  minSize: string;
  maxSize: string;
}

const OTHER_PANE_MIN = "28rem";

const RESIZE_KEYS = new Set(["ArrowLeft", "ArrowRight", "Home", "End", "Enter"]);

const IDLE_FIT_TIMEOUT_MS = 500;

const MARK_LAPSE_MS = 250;

export function useSplitPane(id: string, size: SidePaneSize, { active }: { active: boolean }) {
  const key = `mxr:split:${id}`;
  const panelRef = usePanelRef();
  const groupRef = useRef<HTMLDivElement | null>(null);
  const [saved] = useState(() => readPaneWidth(key));
  const preference = useRef<number | null>(saved);
  const [squeezedMin, setSqueezedMin] = useState<number | null>(null);
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
  const latestFit = useRef(fit);
  useLayoutEffect(() => {
    latestFit.current = fit;
  }, [fit]);
  useLayoutEffect(() => {
    const frame = requestAnimationFrame(() => latestFit.current());
    return () => cancelAnimationFrame(frame);
  }, [squeezedMin]);

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
    if (resizing.current === null && groupRef.current?.querySelector('[data-separator="active"]')) {
      resizing.current = groupWidth();
    }
  }, []);
  const onLayoutChanged = useCallback(() => {
    const panel = panelRef.current;
    const startedAt = resizing.current;
    resizing.current = null;
    if (!active || !panel || startedAt === null || startedAt !== groupWidth()) return;
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
      defaultSize: saved === null ? size.defaultSize : `${saved}px`,
      minSize: squeezedMin === null ? size.minSize : `${squeezedMin}px`,
      maxSize: size.maxSize,
      groupResizeBehavior: "preserve-pixel-size" as const,
    },
    otherPanelProps: { minSize: OTHER_PANE_MIN },
    handleProps: {
      disableDoubleClick: true,
      onDoubleClick,
      onPointerDownCapture: () => startResizing("pointerup"),
      onKeyDownCapture: (event: KeyboardEvent) => {
        if (RESIZE_KEYS.has(event.key)) startResizing("keydown");
      },
    },
  };
}

function whenIdle(run: () => void): { cancel: () => void } {
  if ("requestIdleCallback" in window) {
    const handle = window.requestIdleCallback(run, { timeout: IDLE_FIT_TIMEOUT_MS });
    return { cancel: () => window.cancelIdleCallback(handle) };
  }
  const timer = globalThis.setTimeout(run, 0);
  return { cancel: () => globalThis.clearTimeout(timer) };
}

function toPx(value: string, groupWidth: number): number {
  const amount = Number.parseFloat(value);
  if (value.endsWith("rem")) {
    return amount * Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  }
  if (value.endsWith("px")) return amount;
  return (amount / 100) * groupWidth;
}
