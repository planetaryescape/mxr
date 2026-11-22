import { CircleCheck, CircleX, Info, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast, Toaster as SonnerToaster, useSonner } from "sonner";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { cornerAvoiding, type ToastCorner } from "./toastPlacement";

export const TOAST_DURATION_MS = 5_000;
export const UNDO_TOAST_DURATION_MS = 8_000;

const BELOW_TOPBAR = "calc(var(--shell-topbar-h) + 0.75rem)";
const ABOVE_STATUSBAR = "calc(var(--shell-statusbar-h) + 0.75rem)";
const ABOVE_MOBILE_TABS = "calc(env(safe-area-inset-bottom, 0px) + 4.5rem)";

const ICONS = {
  success: <CircleCheck className="size-4" aria-hidden />,
  info: <Info className="size-4" aria-hidden />,
  warning: <TriangleAlert className="size-4" aria-hidden />,
  error: <CircleX className="size-4" aria-hidden />,
};

const KEY_HINT_ID = "key-hint";

const MOBILE_WIDTH = 600;

function pxVar(name: string, fallback: number): number {
  const value = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  return Number.isFinite(value) ? value : fallback;
}

function litRowBox(): DOMRect | null {
  return document.querySelector<HTMLElement>('[data-lit="true"]')?.getBoundingClientRect() ?? null;
}

function useOneAtATime(preferred: ToastCorner): ToastCorner {
  const toasts = useSonner().toasts.filter((item) => item.id !== KEY_HINT_ID);
  const [corner, setCorner] = useState(preferred);
  const newest = toasts[0]?.id;
  useEffect(() => {
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
    <div ref={container} className="contents">
      <SonnerToaster
        theme={resolved}
        position={position}
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

function stack(target: EventTarget | null): HTMLElement | null {
  return target instanceof HTMLElement
    ? target.closest<HTMLElement>("[data-sonner-toaster]")
    : null;
}

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
