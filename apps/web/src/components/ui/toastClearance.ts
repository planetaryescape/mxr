/*
 * Toasts never cover a primary action. Bars that hold one (focus mode's
 * queue keys, the composer's Send row, the bulk bar) register through
 * `useToastKeepClear`; while a toast is showing, the toast stack is lifted
 * above any such bar it would overlap. The lift is a CSS variable on <html>
 * that the Toaster's offsets read, so nothing re-renders. With no toast on
 * screen nothing is observed or measured: a bar mounting only joins a set.
 */

import { useCallback, type RefCallback } from "react";

const LIFT_VAR = "--toast-clear-bottom";
const GAP = 8;

interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/**
 * How far above the viewport's bottom edge the toast stack must sit so that
 * a stack `stackHeight` tall, spanning `left`..`right`, overlaps none of
 * `obstacles`. Starts at `base` (the toaster's own offset) and lifts above
 * each obstacle it hits, in turn. Returns `base` when nothing is in the way,
 * and gives up (returns `base`) if clearing would push it off the top.
 */
export function toastLift(
  viewportHeight: number,
  stack: { left: number; right: number; height: number },
  obstacles: readonly Box[],
  base: number,
): number {
  let lift = base;
  for (let round = 0; round <= obstacles.length; round += 1) {
    const bottom = viewportHeight - lift;
    const top = bottom - stack.height;
    const hit = obstacles.find(
      (box) =>
        box.right > stack.left &&
        box.left < stack.right &&
        box.bottom > top - GAP &&
        box.top < bottom + GAP,
    );
    if (!hit) return lift;
    lift = viewportHeight - hit.top + GAP;
    if (lift + stack.height > viewportHeight) return base;
  }
  return lift;
}

/** Bars a toast must not cover, and the watcher to tell while a toast shows. */
const keepClear = new Set<HTMLElement>();
let onKeepClearChange: ((element: HTMLElement, added: boolean) => void) | null = null;

/**
 * A ref callback for a bar holding a primary action: a toast never covers
 * it, including a bar that mounts or resizes while the toast is up.
 */
export function useToastKeepClear(): RefCallback<HTMLElement> {
  return useCallback((element: HTMLElement | null) => {
    if (!element) return;
    keepClear.add(element);
    onKeepClearChange?.(element, true);
    return () => {
      keepClear.delete(element);
      onKeepClearChange?.(element, false);
    };
  }, []);
}

/** The toaster's resting bottom offsets, as passed to sonner (desktop, and at most 600 wide). */
export interface ToastOffsets {
  bottom: number;
  mobileBottom: number;
}

/**
 * Keep the toast stack in `container` clear of every keep-clear bar while a
 * toast shows. A toast arriving or leaving measures once; resize and scroll
 * listeners exist only while one is on screen. Returns the cleanup.
 */
export function watchToastClearance(container: HTMLElement, offsets: ToastOffsets): () => void {
  const root = document.documentElement;
  let frame = 0;
  let written: string | null = null;
  let listening = false;

  const write = (value: string | null) => {
    if (value === written) return;
    written = value;
    if (value === null) root.style.removeProperty(LIFT_VAR);
    else root.style.setProperty(LIFT_VAR, value);
  };

  // Bars resizing (a bulk bar wrapping) or mounting while a toast shows.
  let bars: ResizeObserver | null = null;
  const listen = (on: boolean) => {
    if (on === listening) return;
    listening = on;
    const method = on ? "addEventListener" : "removeEventListener";
    window[method]("resize", schedule);
    window[method]("scroll", schedule, { capture: true });
    if (on) {
      bars = new ResizeObserver(schedule);
      for (const element of keepClear) bars.observe(element);
      onKeepClearChange = (element, added) => {
        if (added) bars?.observe(element);
        else bars?.unobserve(element);
        schedule();
      };
    } else {
      bars?.disconnect();
      bars = null;
      onKeepClearChange = null;
    }
  };

  const measure = () => {
    const front = container.querySelector<HTMLElement>("[data-sonner-toast][data-front='true']");
    listen(front !== null);
    if (!front) {
      write(null);
      return;
    }
    const obstacles = [...keepClear]
      .map((element) => element.getBoundingClientRect())
      .filter((box) => box.width > 0 && box.height > 0);
    const stack = front.getBoundingClientRect();
    const base = window.innerWidth <= 600 ? offsets.mobileBottom : offsets.bottom;
    const lift = toastLift(window.innerHeight, stack, obstacles, base);
    write(lift === base ? null : `${lift}px`);
  };

  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      measure();
    });
  }

  const observer = new MutationObserver(schedule);
  // Subtree: the toast list itself mounts with the first toast.
  observer.observe(container, { childList: true, subtree: true });
  return () => {
    observer.disconnect();
    listen(false);
    if (frame) cancelAnimationFrame(frame);
    write(null);
  };
}
