import { Download, X } from "lucide-react";
import { useState } from "react";

import {
  isStandalone,
  promptInstall,
  readDismissed,
  shouldOfferInstall,
  useInstallPrompt,
  writeDismissed,
} from "./installPrompt";

/**
 * A small offer in the status bar, shown only when the browser can install
 * the app. It is not a banner: it sits beside the other status items and
 * goes away when dismissed, and the dismissal is remembered.
 */
export function InstallOffer() {
  const installable = useInstallPrompt((s) => s.deferred !== null);
  const [dismissed, setDismissed] = useState(readDismissed);
  const visible = shouldOfferInstall({
    installable,
    standalone: isStandalone(),
    dismissed,
  });
  if (!visible) return null;

  const dismiss = () => {
    writeDismissed();
    setDismissed(true);
  };

  return (
    <span className="inline-flex shrink-0 items-center gap-1" data-testid="install-offer">
      <button
        type="button"
        onClick={() => void promptInstall()}
        className="inline-flex items-center gap-1 rounded-sm px-1 text-foreground hover:bg-muted"
        aria-label="Install mxr as an app"
        title="Open mxr in its own window, with its own keys"
      >
        <Download className="size-3" aria-hidden />
        install app
      </button>
      <button
        type="button"
        onClick={dismiss}
        className="grid size-4 place-items-center rounded-sm text-text-3 hover:text-text-1"
        aria-label="Hide the install offer"
        title="Hide the install offer"
      >
        <X className="size-3" aria-hidden />
      </button>
    </span>
  );
}
