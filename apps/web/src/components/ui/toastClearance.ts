/*
 * Toasts never cover a primary action. Bars that hold one (focus mode's
 * queue keys, the composer's Send row, the bulk bar) carry
 * `data-toast-keep-clear`; while a toast is showing, the toast stack is
 * lifted above any such bar it would overlap. The lift is a CSS variable on
 * <html> that the Toaster's offsets read, so nothing re-renders, and the
 * work only runs while a toast is on screen.
 */

const KEEP_CLEAR_SELECTOR = "[data-toast-keep-clear]";
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

  const listen = (on: boolean) => {
    if (on === listening) return;
    listening = on;
    const method = on ? "addEventListener" : "removeEventListener";
    window[method]("resize", schedule);
    window[method]("scroll", schedule, { capture: true });
  };

  const measure = () => {
    const front = container.querySelector<HTMLElement>("[data-sonner-toast][data-front='true']");
    listen(front !== null);
    if (!front) {
      write(null);
      return;
    }
    const obstacles = [...document.querySelectorAll<HTMLElement>(KEEP_CLEAR_SELECTOR)]
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
