import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Alert } from "@/components/ui/alert";
import { useDaemonDown } from "@/lib/daemonAvailability";
import { useConnectionStore } from "@/state/connectionStore";

/** The event stream alone dropping only matters once it stays down. */
const STREAM_BANNER_DELAY_MS = 30_000;

export function OfflineBanner() {
  const state = useConnectionStore((s) => s.state);
  const protocolMismatch = useConnectionStore((s) => s.protocolMismatch);
  const daemonDown = useDaemonDown();
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

  // Calm, not an alarm: nothing the person has on screen is lost.
  const banner = daemonDown
    ? {
        kind: "daemon",
        text: "Could not reach mxr. Mail already loaded on this page may still be available, but new pages and changes need a connection. Check the connection, or run mxr restart on the computer running mxr.",
      }
    : offlineSince && now - offlineSince >= STREAM_BANNER_DELAY_MS
      ? {
          kind: "stream",
          text: "Live updates are paused while mxr reconnects. Mail still loads; new mail shows up once it's back.",
        }
      : null;
  if (!banner) return null;

  return (
    <Alert
      data-offline-banner={banner.kind}
      role="status"
      variant="warning"
      className="rounded-none border-x-0 border-t-0 px-4 py-2 text-xs text-foreground"
    >
      {banner.text}
      {banner.kind === "daemon" ? (
        <button
          type="button"
          className="ml-2 underline underline-offset-2"
          onClick={() => {
            if (!navigator.clipboard) {
              toast.error("Copy is unavailable in this browser");
              return;
            }
            void navigator.clipboard
              .writeText("mxr restart")
              .then(() => toast.success("Restart command copied"))
              .catch(() => toast.error("Could not copy the command"));
          }}
        >
          Copy command
        </button>
      ) : null}
    </Alert>
  );
}
