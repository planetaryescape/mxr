/*
 * A toast never covers the lit row (D140). It sits in its corner unless the
 * lit row is under where it would draw, and then it takes the other corner.
 * The geometry is computed, not measured from the toast, so the choice is
 * made before the toast has been placed and never makes it jump.
 */

export type ToastCorner = "top-right" | "bottom-right";

interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Sonner's toast width, and the gutter the Toaster leaves at the viewport edge. */
const TOAST_WIDTH = 356;
const GUTTER = 16;
/** A one-line toast with its Undo; a two-line one is taller, so leave room. */
const TOAST_HEIGHT = 72;

interface Viewport {
  width: number;
  height: number;
  /** Distance from the corner's edge: below the top bar, above the status bar or tabs. */
  top: number;
  bottom: number;
}

function toastBox(corner: ToastCorner, view: Viewport): Box {
  const width = Math.min(TOAST_WIDTH, view.width - GUTTER * 2);
  const right = view.width - GUTTER;
  if (corner === "top-right") {
    return { left: right - width, right, top: view.top, bottom: view.top + TOAST_HEIGHT };
  }
  const bottom = view.height - view.bottom;
  return { left: right - width, right, top: bottom - TOAST_HEIGHT, bottom };
}

const overlaps = (a: Box, b: Box) =>
  a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;

/** The corner to draw in: `preferred`, unless the lit row is under it and the other is clear. */
export function cornerAvoiding(
  preferred: ToastCorner,
  litRow: Box | null,
  view: Viewport,
): ToastCorner {
  if (!litRow || !overlaps(toastBox(preferred, view), litRow)) return preferred;
  const other = preferred === "top-right" ? "bottom-right" : "top-right";
  return overlaps(toastBox(other, view), litRow) ? preferred : other;
}
