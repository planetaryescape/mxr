import { useEffect, useRef } from "react";
import { Toaster as SonnerToaster } from "sonner";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { watchToastClearance } from "./toastClearance";

// Clear of the status bar; sonner's own 16px on a phone.
const OFFSETS = { bottom: 40, mobileBottom: 16 };

export function Toaster() {
  const theme = useUiPrefs((s) => s.theme);
  const resolved =
    theme === "system" ? "system" : theme === "light" || theme === "paper" ? "light" : "dark";
  const container = useRef<HTMLDivElement>(null);
  useEffect(
    () => (container.current ? watchToastClearance(container.current, OFFSETS) : undefined),
    [],
  );
  return (
    // `contents`: the wrapper only gives the clearance watcher a root.
    <div ref={container} className="contents">
      <SonnerToaster
        theme={resolved}
        // Bottom-right like the TUI's toast stack, clear of the status bar.
        position="bottom-right"
        // The lift (toastClearance.ts) replaces the resting offset while a
        // toast would cover a primary action.
        offset={{ bottom: `var(--toast-clear-bottom, ${OFFSETS.bottom}px)`, right: 16 }}
        mobileOffset={{ bottom: `var(--toast-clear-bottom, ${OFFSETS.mobileBottom}px)` }}
        duration={4_000}
        visibleToasts={4}
        closeButton
        toastOptions={{
          classNames: {
            toast:
              "rounded-md border border-border-strong bg-popover text-popover-foreground shadow-xl",
            title: "text-[13px] font-medium",
            description: "text-2xs text-muted-foreground",
            actionButton: "bg-primary text-primary-foreground hover:bg-primary/90",
            cancelButton: "bg-muted text-muted-foreground",
          },
        }}
      />
    </div>
  );
}
