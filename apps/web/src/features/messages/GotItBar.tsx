import { Undo2 } from "lucide-react";
import { useEffect, useState } from "react";

import { KeyChip } from "@/components/KeyChip";

import { countdownLabel } from "./messagesView";
import type { PendingAck } from "./useGotIt";

/**
 * Got it's preview: the exact text that will be sent, who it goes to, a
 * visible countdown and undo. Nothing leaves until the countdown ends.
 */
export function GotItBar({ pending, onUndo }: { pending: PendingAck; onUndo: () => void }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const tick = window.setInterval(() => setNow(Date.now()), 200);
    return () => window.clearInterval(tick);
  }, []);
  const total = pending.plan.countdown_seconds * 1000;
  const left = Math.max(0, pending.endsAt - now);
  const to = pending.plan.to
    .map((address) => (address.name ? `${address.name} <${address.email}>` : address.email))
    .join(", ");
  return (
    <section
      aria-label="Got it, about to send"
      data-testid="got-it-preview"
      className="mx-4 mb-3 rounded-none border border-primary/50 bg-surface px-4 py-3"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <div className="min-w-0 text-xs text-muted-foreground">
          <p>
            Got it to <span className="break-words text-foreground">{to || "No recipient"}</span>
          </p>
          <p className="mt-0.5">
            From <span className="break-words text-foreground">{pending.plan.from}</span>
          </p>
        </div>
        <p
          data-testid="got-it-countdown"
          role="timer"
          aria-live="off"
          className="font-mono text-xs tabular-nums text-muted-foreground"
        >
          {countdownLabel(pending.endsAt, now)}
        </p>
      </div>
      <p
        data-testid="got-it-text"
        className="mt-1.5 whitespace-pre-wrap text-sm leading-6 text-foreground"
      >
        {pending.plan.text}
      </p>
      <div className="mt-2 h-0.5 overflow-hidden rounded-sm bg-border" aria-hidden>
        <div
          className="h-full origin-left bg-primary"
          style={{ transform: `scaleX(${total > 0 ? left / total : 0})` }}
        />
      </div>
      <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">{pending.plan.built_from}</p>
        <button
          type="button"
          onClick={onUndo}
          data-testid="got-it-undo"
          className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-foreground hover:bg-accent"
        >
          <Undo2 className="size-3.5" aria-hidden /> Undo <KeyChip className="h-4 px-1">u</KeyChip>
        </button>
      </div>
    </section>
  );
}
