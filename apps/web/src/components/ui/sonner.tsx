import { CircleCheck, CircleX, Info, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast, Toaster as SonnerToaster, useSonner } from "sonner";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { cornerAvoiding, type ToastCorner } from "./toastPlacement";

/** D140: a toast goes after 5 seconds, 8 when it carries an Undo. The independent keyboard undo stays live. */
export const TOAST_DURATION_MS = 5_000;
export const UNDO_TOAST_DURATION_MS = 8_000;

/* Keep quiet feedback below Search and Compose in the app header. */
const BELOW_TOPBAR = "calc(var(--shell-topbar-h) + 0.75rem)";
/* Keep normal-mail feedback above the bottom status and mobile tabs. */
const ABOVE_STATUSBAR = "calc(var(--shell-statusbar-h) + 0.75rem)";
const ABOVE_MOBILE_TABS = "calc(env(safe-area-inset-bottom, 0px) + 4.5rem)";

const ICONS = {
  success: <CircleCheck className="size-4" aria-hidden />,
  info: <Info className="size-4" aria-hidden />,
  warning: <TriangleAlert className="size-4" aria-hidden />,
  error: <CircleX className="size-4" aria-hidden />,
};

/** Sonner switches to its mobile layout at this width. */
const MOBILE_WIDTH = 600;

function pxVar(name: string, fallback: number): number {
  const value = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  return Number.isFinite(value) ? value : fallback;
}

/** The lit row's box, if a row is lit. */
function litRowBox(): DOMRect | null {
  return document.querySelector<HTMLElement>('[data-lit="true"]')?.getBoundingClientRect() ?? null;
}

/**
 * One toast at a time: a new one replaces the one before (the activity log
 * keeps them all), and it takes the corner that leaves the lit row clear.
 */
function useOneAtATime(preferred: ToastCorner): ToastCorner {
  const { toasts } = useSonner();
  const [corner, setCorner] = useState(preferred);
  const newest = toasts[0]?.id;
  useEffect(() => {
    // Sonner lists the newest first.
    for (const older of toasts.slice(1)) toast.dismiss(older.id);
  }, [toasts]);
  useEffect(() => {
    if (newest === undefined) {
      setCorner(preferred);
      return;
    }
    const mobile = window.innerWidth < MOBILE_WIDTH;
    setCorner(
      cornerAvoiding(preferred, litRowBox(), {
        width: window.innerWidth,
        height: window.innerHeight,
        top: pxVar("--shell-topbar-h", 48) + 12,
        bottom: mobile ? 72 : pxVar("--shell-statusbar-h", 28) + 12,
      }),
    );
  }, [newest, preferred]);
  return corner;
}

export function Toaster({ position: preferred = "bottom-right" }: { position?: ToastCorner }) {
  const position = useOneAtATime(preferred);
  const theme = useUiPrefs((s) => s.theme);
  const resolved =
    theme === "system" ? "system" : theme === "light" || theme === "paper" ? "light" : "dark";
  const container = useRef<HTMLDivElement>(null);
  useEffect(() => (container.current ? pauseOnFocus(container.current) : undefined), []);
  return (
    // `contents`: the wrapper only gives the focus listener a root.
    <div ref={container} className="contents">
      <SonnerToaster
        theme={resolved}
        position={position}
        // Colour by type (app.css maps sonner's rich colours to theme tokens).
        richColors
        offset={position === "top-right" ? { top: BELOW_TOPBAR } : { bottom: ABOVE_STATUSBAR }}
        mobileOffset={
          position === "top-right" ? { top: BELOW_TOPBAR } : { bottom: ABOVE_MOBILE_TABS }
        }
        duration={TOAST_DURATION_MS}
        visibleToasts={1}
        closeButton
        icons={ICONS}
        toastOptions={{
          classNames: {
            title: "text-sm font-medium",
            description: "text-2xs",
          },
        }}
      />
      <AssertiveErrors />
    </div>
  );
}

/**
 * Sonner's region is polite. An error says itself again here, assertively,
 * so a screen reader hears it at once rather than after whatever it is
 * reading.
 */
function AssertiveErrors() {
  const { toasts } = useSonner();
  const latest = toasts.find((item) => item.type === "error");
  const text = latest && typeof latest.title === "string" ? latest.title : "";
  return (
    <div aria-live="assertive" aria-atomic="true" className="sr-only">
      {text}
    </div>
  );
}

/** The toast stack holding `target`, if it is in one. */
function stack(target: EventTarget | null): HTMLElement | null {
  return target instanceof HTMLElement
    ? target.closest<HTMLElement>("[data-sonner-toaster]")
    : null;
}

/**
 * Sonner pauses a toast's timer while the pointer is over the stack but not
 * while keyboard focus is in it. Focus entering the stack sends the same
 * mouse event sonner listens for, and leaving sends the mouse-out, so
 * someone tabbing to Undo has the time they need.
 */
function pauseOnFocus(root: HTMLElement): () => void {
  const onFocusIn = (event: FocusEvent) => {
    stack(event.target)?.dispatchEvent(new MouseEvent("mousemove", { bubbles: true }));
  };
  const onFocusOut = (event: FocusEvent) => {
    const list = stack(event.target);
    if (!list || list.contains(event.relatedTarget as Node | null)) return;
    list.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
  };
  root.addEventListener("focusin", onFocusIn);
  root.addEventListener("focusout", onFocusOut);
  return () => {
    root.removeEventListener("focusin", onFocusIn);
    root.removeEventListener("focusout", onFocusOut);
  };
}

/** The Undo button's label, with the key that does the same. */
export function UndoLabel() {
  return (
    <span className="inline-flex items-center gap-1.5">
      Undo
      <kbd aria-hidden className="font-mono text-2xs">
        u
      </kbd>
    </span>
  );
}
