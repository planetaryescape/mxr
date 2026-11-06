import { useEffect, useRef, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { useDaemonDown } from "@/lib/daemonAvailability";
import { copyCommand, START_COMMAND } from "@/lib/copyCommand";
import { cn } from "@/lib/utils";
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

/** A floating notice: the shell never makes room for it, so nothing moves. */
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
      className="type-body pointer-events-auto flex w-full max-w-xl flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border-strong bg-ink-lamp px-4 py-2.5 text-text-1 shadow-xl"
    >
      <span aria-hidden className="size-1.5 shrink-0 bg-cue" />
      <span className="min-w-0 flex-1">{children}</span>
      {action}
    </div>
  );
}

/** How long a leaving banner is held so it can fade (the fast motion duration). */
const FADE_OUT_MS = 120;

/**
 * The banner renders nothing when all is well, so a fade out needs the last
 * banner kept on screen while it goes: it fades in on mount and, once there
 * is nothing to say, fades out and unmounts. Reduced motion keeps the fades.
 *
 * It floats under the top bar instead of sitting in the main column's flow:
 * a daemon outage or a paused stream must not shove the list and reader down
 * the moment it starts, or back up when it ends. The band still covers the
 * page's own header while it shows, but no mail row moves.
 */
export function OfflineBanner() {
  const banner = useBannerContent();
  const present = banner !== null;
  // The last banner, kept on screen while it fades out.
  const last = useRef<ReactNode>(null);
  if (banner) last.current = banner;
  const [gone, setGone] = useState(!present);
  useEffect(() => {
    if (present) {
      setGone(false);
      return;
    }
    const timer = window.setTimeout(() => setGone(true), FADE_OUT_MS);
    return () => window.clearTimeout(timer);
  }, [present]);
  const shown = banner ?? (gone ? null : last.current);
  if (!shown) return null;
  return (
    <div
      className={cn(
        "pointer-events-none absolute inset-x-3 top-3 z-30 flex justify-center duration-fast",
        present ? "animate-in fade-in-0" : "animate-out fade-out-0 fill-mode-forwards",
      )}
    >
      {shown}
    </div>
  );
}

function useBannerContent(): ReactNode {
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
