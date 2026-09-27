/*
 * The undo-send toast's live parts: seconds left (tabular, so the digits
 * don't jiggle) and a thin bar that drains over the window. The bar is a
 * transform animation started once; the number ticks from the deadline, so
 * a slow frame never makes it drift from the real send time.
 */

import { useEffect, useState } from "react";
import { toast } from "sonner";

export function SendCountdownTitle({ deadline }: { deadline: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, []);
  const seconds = Math.max(0, Math.ceil((deadline - now) / 1000));
  return (
    <span data-testid="send-countdown" className="tabular-nums">
      {seconds > 0 ? `Sending in ${seconds}s` : "Sending…"}
    </span>
  );
}

export function SendCountdownBar({ seconds }: { seconds: number }) {
  return (
    <span aria-hidden className="mt-2 block h-0.5 w-full overflow-hidden rounded-full bg-muted">
      <span
        data-testid="send-countdown-bar"
        className="countdown-drain block h-full w-full bg-primary"
        style={{ "--countdown-duration": `${seconds}s` } as React.CSSProperties}
      />
    </span>
  );
}

/** The undo-send toast: live seconds, the draining bar, Undo. Returns the
 * toast id so the send can dismiss it the moment it fires. */
export function showSendCountdown({
  seconds,
  summary,
  onUndo,
}: {
  seconds: number;
  summary: string;
  onUndo: () => void;
}): string | number {
  const deadline = Date.now() + seconds * 1000;
  return toast(<SendCountdownTitle deadline={deadline} />, {
    duration: seconds * 1000,
    description: (
      <>
        <span className="block">{summary}</span>
        <SendCountdownBar seconds={seconds} />
      </>
    ),
    action: { label: "Undo", onClick: onUndo },
  });
}
