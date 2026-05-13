import { Toaster as SonnerToaster } from "sonner";

import { useUiPrefs } from "@/state/uiPrefsStore";

export function Toaster() {
  const theme = useUiPrefs((s) => s.theme);
  const resolved =
    theme === "system" ? "system" : theme === "light" || theme === "paper" ? "light" : "dark";
  return (
    <SonnerToaster
      theme={resolved}
      position="top-right"
      duration={4_000}
      closeButton
      toastOptions={{
        classNames: {
          toast: "rounded-md border border-border bg-popover text-popover-foreground shadow-lg",
          title: "text-sm font-medium",
          description: "text-2xs text-muted-foreground",
          actionButton: "bg-primary text-primary-foreground hover:bg-primary/90",
          cancelButton: "bg-muted text-muted-foreground",
        },
      }}
    />
  );
}
