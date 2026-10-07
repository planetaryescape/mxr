/*
 * The swipe gesture on list rows, for touch only. One set of listeners on
 * the list (delegation), not per row, and every callback is read through a
 * ref: the list and its virtualizer never see a new function, and nothing
 * re-renders while a finger moves. The row follows the finger through its
 * own style; only the layer under it (colour, icon, words) re-renders, and
 * only when the pending action changes.
 */

import { Archive, Bookmark, Check, Clock, Trash2, type LucideIcon } from "lucide-react";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";

import { cn } from "@/lib/utils";

import {
  followDistance,
  lockAxis,
  pendingAt,
  releaseAt,
  SWIPE_WORDS,
  type Pending,
  type SwipeAction,
  type SwipeMap,
} from "./swipeRules";

export interface SwipeTarget {
  actions: SwipeMap;
  commit: (action: SwipeAction) => void;
}

export interface SwipeLayerHandle {
  show: (row: HTMLElement, pending: Pending | null) => void;
  hide: () => void;
}

const COARSE_POINTER = "(pointer: coarse)";

const ICONS: Record<SwipeAction, LucideIcon> = {
  archive: Archive,
  done: Check,
  trash: Trash2,
  snooze: Clock,
  sweep: Archive,
  later: Bookmark,
  letgo: Check,
};

const TONES: Record<SwipeAction, string> = {
  archive: "swipe-tone-primary",
  done: "swipe-tone-primary",
  sweep: "swipe-tone-primary",
  snooze: "swipe-tone-quiet",
  trash: "swipe-tone-destructive",
  later: "swipe-tone-quiet",
  letgo: "swipe-tone-primary",
};

/**
 * What shows under a moving row. Portaled into a host placed just before the
 * row, sized to it, so the row (later in the DOM, positioned) paints on top.
 */
export const SwipeLayer = forwardRef<SwipeLayerHandle>(function SwipeLayer(_props, ref) {
  const [state, setState] = useState<{ host: HTMLElement; pending: Pending | null } | null>(null);
  const hostRef = useRef<HTMLElement | null>(null);

  useImperativeHandle(ref, () => ({
    show: (row, pending) => {
      let host = hostRef.current;
      if (!host || host.nextSibling !== row) {
        host?.remove();
        host = document.createElement("div");
        host.setAttribute("data-testid", "swipe-layer");
        host.setAttribute("aria-hidden", "true");
        row.before(host);
        hostRef.current = host;
      }
      Object.assign(host.style, {
        position: "absolute",
        top: `${row.offsetTop}px`,
        left: `${row.offsetLeft}px`,
        width: `${row.offsetWidth}px`,
        height: `${row.offsetHeight}px`,
      });
      setState((current) =>
        current?.host === host && samePending(current.pending, pending)
          ? current
          : { host, pending },
      );
    },
    hide: () => {
      hostRef.current?.remove();
      hostRef.current = null;
      setState(null);
    },
  }));

  useEffect(() => () => hostRef.current?.remove(), []);

  if (!state) return null;
  const { pending } = state;
  const Icon = pending ? ICONS[pending.action] : null;
  return createPortal(
    <div
      data-action={pending?.action}
      data-armed={pending?.armed ? "true" : undefined}
      className={cn(
        "swipe-layer flex h-full w-full items-center rounded-md px-5 text-[12.5px]",
        pending ? TONES[pending.action] : "bg-muted/40",
        pending?.side === "left" ? "justify-end" : "justify-start",
      )}
    >
      {pending && Icon ? (
        <span className="inline-flex items-center gap-1.5">
          <Icon className="size-4" />
          {SWIPE_WORDS[pending.action]}
        </span>
      ) : null}
    </div>,
    state.host,
  );
});

function samePending(a: Pending | null, b: Pending | null): boolean {
  return a?.action === b?.action && a?.side === b?.side && a?.armed === b?.armed;
}

interface Gesture {
  row: HTMLElement;
  target: SwipeTarget;
  pointerId: number;
  startX: number;
  startY: number;
  axis: "pending" | "x" | "y";
  dx: number;
  lastX: number;
  lastT: number;
  velocity: number;
  background: { color: string; image: string };
}

/**
 * Swipe rows inside `container`. `rowSelector` finds the row under a touch;
 * `resolve` says what that row does (null: not swipeable).
 */
export function useRowSwipe(
  container: RefObject<HTMLElement | null>,
  options: {
    rowSelector: string;
    resolve: (row: HTMLElement) => SwipeTarget | null;
    layer: RefObject<SwipeLayerHandle | null>;
  },
  /** False while the list isn't rendered (loading, empty). */
  enabled = true,
): void {
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    const root = container.current;
    if (!enabled || !root || typeof window.matchMedia !== "function") return;
    let gesture: Gesture | null = null;
    let suppressClickUntil = 0;
    const reduced = () => document.documentElement.dataset.motion === "reduced";

    const restore = (row: HTMLElement, background: Gesture["background"]) => {
      row.style.transform = "";
      row.style.transition = "";
      row.style.backgroundColor = background.color;
      row.style.backgroundImage = background.image;
      row.style.zIndex = "";
    };

    const onDown = (event: PointerEvent) => {
      if (event.pointerType !== "touch" || !window.matchMedia(COARSE_POINTER).matches) return;
      if (gesture || !(event.target instanceof Element)) return;
      const row = event.target.closest<HTMLElement>(optionsRef.current.rowSelector);
      if (!row || !root.contains(row)) return;
      const target = optionsRef.current.resolve(row);
      if (!target) return;
      gesture = {
        row,
        target,
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        axis: "pending",
        dx: 0,
        lastX: event.clientX,
        lastT: event.timeStamp,
        velocity: 0,
        background: { color: row.style.backgroundColor, image: row.style.backgroundImage },
      };
    };

    const onMove = (event: PointerEvent) => {
      const active = gesture;
      if (!active || event.pointerId !== active.pointerId) return;
      const dx = event.clientX - active.startX;
      const dy = event.clientY - active.startY;
      if (active.axis === "pending") {
        active.axis = lockAxis(dx, dy);
        if (active.axis === "y") {
          gesture = null;
          return;
        }
        if (active.axis === "pending") return;
        // Opaque while it moves, keeping any selection tint on top.
        const tint = getComputedStyle(active.row).backgroundColor;
        active.row.style.backgroundImage = `linear-gradient(${tint}, ${tint})`;
        active.row.style.backgroundColor = "var(--background)";
        active.row.style.transition = "none";
      }
      const elapsed = Math.max(1, event.timeStamp - active.lastT);
      active.velocity = (event.clientX - active.lastX) / elapsed;
      active.lastX = event.clientX;
      active.lastT = event.timeStamp;
      active.dx = dx;
      active.row.style.transform = `translate3d(${followDistance(dx, active.target.actions)}px, 0, 0)`;
      optionsRef.current.layer.current?.show(
        active.row,
        pendingAt(dx, active.velocity, active.target.actions),
      );
    };

    const finish = (event: PointerEvent, cancelled: boolean) => {
      const active = gesture;
      if (!active || event.pointerId !== active.pointerId) return;
      gesture = null;
      if (active.axis !== "x") return;
      suppressClickUntil = event.timeStamp + 400;
      const action = cancelled
        ? null
        : releaseAt(active.dx, active.velocity, active.target.actions);
      const { row } = active;
      const layer = optionsRef.current.layer.current;
      const settle = () => {
        restore(row, active.background);
        layer?.hide();
      };
      if (!action) {
        if (reduced()) {
          settle();
          return;
        }
        row.style.transition = "transform var(--motion-duration-base) var(--ease-out)";
        row.style.transform = "translate3d(0, 0, 0)";
        window.setTimeout(settle, 200);
        return;
      }
      // Later keeps the item where it is; let go and the mail verbs take it.
      const leaves = action !== "snooze" && action !== "sweep" && action !== "later";
      if (!leaves || reduced()) {
        settle();
        active.target.commit(action);
        return;
      }
      // Off it goes in the direction of the throw, then the list takes it.
      row.style.transition = "transform var(--motion-duration-fast) var(--ease-out)";
      row.style.transform = `translate3d(${Math.sign(active.dx) * row.offsetWidth}px, 0, 0)`;
      window.setTimeout(() => {
        active.target.commit(action);
        // If the row is still here (a failed or confirmed action), put it back.
        window.setTimeout(settle, 250);
      }, 120);
    };
    const onUp = (event: PointerEvent) => finish(event, false);
    const onCancel = (event: PointerEvent) => finish(event, true);
    const onClick = (event: MouseEvent) => {
      if (event.timeStamp < suppressClickUntil) {
        event.preventDefault();
        event.stopPropagation();
      }
    };

    root.addEventListener("pointerdown", onDown);
    root.addEventListener("pointermove", onMove);
    root.addEventListener("pointerup", onUp);
    root.addEventListener("pointercancel", onCancel);
    root.addEventListener("click", onClick, true);
    return () => {
      root.removeEventListener("pointerdown", onDown);
      root.removeEventListener("pointermove", onMove);
      root.removeEventListener("pointerup", onUp);
      root.removeEventListener("pointercancel", onCancel);
      root.removeEventListener("click", onClick, true);
    };
  }, [container, enabled]);
}
