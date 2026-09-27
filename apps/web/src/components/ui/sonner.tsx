import { Toaster as SonnerToaster } from "sonner";

import { useUiPrefs } from "@/state/uiPrefsStore";

export function Toaster() {
  const theme = useUiPrefs((s) => s.theme);
  const resolved =
    theme === "system" ? "system" : theme === "light" || theme === "paper" ? "light" : "dark";
  return (
    <SonnerToaster
      theme={resolved}
      // Bottom-right like the TUI's toast stack, clear of the status bar.
      position="bottom-right"
      offset={{ bottom: 40, right: 16 }}
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
  );
}
