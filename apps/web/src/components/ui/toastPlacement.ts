export type ToastCorner = "top-right" | "bottom-right";

interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

const TOAST_WIDTH = 356;
const GUTTER = 16;
const TOAST_HEIGHT = 72;

interface Viewport {
  width: number;
  height: number;
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

export function cornerAvoiding(
  preferred: ToastCorner,
  litRow: Box | null,
  view: Viewport,
): ToastCorner {
  if (!litRow || !overlaps(toastBox(preferred, view), litRow)) return preferred;
  const other = preferred === "top-right" ? "bottom-right" : "top-right";
  return overlaps(toastBox(other, view), litRow) ? preferred : other;
}
