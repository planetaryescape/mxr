import { useEffect, useState } from "react";

import { Alert } from "@/components/ui/alert";
import { useConnectionStore } from "@/state/connectionStore";

const OFFLINE_BANNER_DELAY_MS = 30_000;

export function OfflineBanner() {
  const state = useConnectionStore((s) => s.state);
  const protocolMismatch = useConnectionStore((s) => s.protocolMismatch);
  const [offlineSince, setOfflineSince] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const disconnected = state === "offline" || state === "reconnecting";

  useEffect(() => {
    if (disconnected) {
      setOfflineSince((value) => value ?? Date.now());
      return;
    }
    setOfflineSince(null);
  }, [disconnected]);

  useEffect(() => {
    if (!disconnected) return;
    const handle = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(handle);
  }, [disconnected]);

  if (protocolMismatch) {
    const actual = protocolMismatch.actualProtocol ?? "missing";
    return (
      <Alert
        data-offline-banner
        role="alert"
        variant="destructive"
        className="rounded-none border-x-0 border-t-0 px-4 py-2 text-xs"
      >
        mxr protocol mismatch. Web expects IPC v{protocolMismatch.requiredProtocol}; bridge reports
        v{actual}. Update mxr: {protocolMismatch.updateSteps.join(" · ")}.
      </Alert>
    );
  }

  if (!offlineSince || now - offlineSince < OFFLINE_BANNER_DELAY_MS) return null;

  return (
    <Alert
      data-offline-banner
      role="alert"
      variant="warning"
      className="rounded-none border-x-0 border-t-0 px-4 py-2 text-xs text-foreground"
    >
      mxr is offline. Changes may be delayed until the daemon reconnects.
    </Alert>
  );
}
