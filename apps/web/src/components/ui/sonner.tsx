import { CircleCheck, CircleX, Info, TriangleAlert } from "lucide-react";
import { useEffect, useRef } from "react";
import { Toaster as SonnerToaster, useSonner } from "sonner";

import { useUiPrefs } from "@/state/uiPrefsStore";

/** The visual undo prompt is brief; the independent keyboard undo stays live. */
export const UNDO_TOAST_DURATION_MS = 8_000;

/* Keep quiet feedback below Search and Compose in the app header. */
const BELOW_TOPBAR = "calc(var(--shell-topbar-h) + 0.75rem)";
/* Keep normal-mail feedback above the bottom status and mobile tabs. */
const ABOVE_STATUSBAR = "calc(var(--shell-statusbar-h) + 0.75rem)";
const ABOVE_MOBILE_TABS = "calc(env(safe-area-inset-bottom, 0px) + 4.5rem)";

type ToastPosition = "top-right" | "bottom-right";

const ICONS = {
  success: <CircleCheck className="size-4" aria-hidden />,
  info: <Info className="size-4" aria-hidden />,
  warning: <TriangleAlert className="size-4" aria-hidden />,
  error: <CircleX className="size-4" aria-hidden />,
};

export function Toaster({ position = "bottom-right" }: { position?: ToastPosition }) {
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
        duration={4_000}
        visibleToasts={3}
        closeButton
        icons={ICONS}
        toastOptions={{
          classNames: {
            title: "text-[13px] font-medium",
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
  const latest = toasts.findLast((toast) => toast.type === "error");
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
      <kbd aria-hidden className="font-mono text-[10.5px]">
        u
      </kbd>
    </span>
  );
}
