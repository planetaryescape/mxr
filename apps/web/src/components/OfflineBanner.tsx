import { useEffect, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { useDaemonDown } from "@/lib/daemonAvailability";
import { copyCommand, START_COMMAND } from "@/lib/copyCommand";
import { useConnectionStore } from "@/state/connectionStore";

/** The event stream alone dropping only matters once it stays down. */
const STREAM_BANNER_DELAY_MS = 30_000;

function clockTime(at: number): string {
  return new Date(at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  });
}

/** The one band under the top bar: what is wrong, and that what is on screen still works. */
function Banner({
  kind,
  role,
  action,
  children,
}: {
  kind: string;
  role: "status" | "alert";
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      data-offline-banner={kind}
      role={role}
      className="type-body flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-rule bg-ink-lamp px-5 py-2 text-text-1"
    >
      <span aria-hidden className="size-1.5 shrink-0 bg-cue" />
      <span className="min-w-0 flex-1">{children}</span>
      {action}
    </div>
  );
}

export function OfflineBanner() {
  const state = useConnectionStore((s) => s.state);
  const protocolMismatch = useConnectionStore((s) => s.protocolMismatch);
  const lastAnsweredAt = useConnectionStore((s) => s.daemonLastAnsweredAt);
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
      <Banner kind="protocol" role="alert">
        mxr needs updating. This web app expects IPC v{protocolMismatch.requiredProtocol}; mxr
        reports v{actual}. Update: {protocolMismatch.updateSteps.join(" · ")}.
      </Banner>
    );
  }

  if (daemonDown) {
    return (
      <Banner
        kind="daemon"
        role="status"
        action={
          <Button variant="outline" size="sm" onClick={() => copyCommand(START_COMMAND)}>
            Copy start command
          </Button>
        }
      >
        mxr isn&apos;t running. Showing what was loaded
        {lastAnsweredAt ? ` at ${clockTime(lastAnsweredAt)}` : ""}.
      </Banner>
    );
  }

  if (offlineSince && now - offlineSince >= STREAM_BANNER_DELAY_MS) {
    return (
      <Banner kind="stream" role="status">
        Live updates are paused while mxr reconnects. Mail still loads; new mail shows up once
        it&apos;s back.
      </Banner>
    );
  }
  return null;
}
