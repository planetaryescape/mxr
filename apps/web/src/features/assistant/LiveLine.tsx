/*
 * The live line: one line above the composer that says what the desk is
 * doing now, rewritten in place, with the seconds the turn has taken and
 * Stop. It exists only while a turn runs, so it can't go stale: the
 * sidebar mounts it from the conversation's own status. A step that runs
 * long names the model, so a slow local one doesn't look stuck.
 */

import { useEffect, useRef, useState } from "react";

import type { AssistantProgress } from "./api";
import { liveLine, slowNote } from "./wire";
import { StatusDot } from "@/components/StatusDot";

export function LiveLine({
  progress,
  model,
  startedAt,
  stopping,
  onStop,
}: {
  progress: AssistantProgress | null;
  model: string | null;
  startedAt: number | null;
  stopping: boolean;
  onStop: () => void;
}) {
  const now = useNow();
  const phrase = liveLine(progress);
  const turnSeconds = startedAt === null ? null : Math.max(0, Math.round((now - startedAt) / 1000));
  const stepSeconds = useStepSeconds(phrase, now);
  const slow = slowNote(model, stepSeconds);
  return (
    <div className="flex items-center gap-2 px-1" role="status" aria-live="polite">
      <StatusDot className="live-dot size-2 bg-signal" />
      <span className="type-meta min-w-0 flex-1 truncate text-text-2">
        {phrase}
        {slow ? <span className="text-cue"> · {slow}</span> : null}
      </span>
      {turnSeconds !== null ? <span className="type-meta text-text-3">{turnSeconds}s</span> : null}
      <button
        type="button"
        onClick={onStop}
        disabled={stopping}
        className="type-meta inline-flex h-6 items-center gap-1 px-1.5 text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1 disabled:opacity-50"
      >
        <span aria-hidden="true">■</span> Stop
      </button>
    </div>
  );
}

function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}

/** Whole seconds since the phrase last changed: the time on this step. */
function useStepSeconds(phrase: string, now: number): number {
  const since = useRef({ phrase, at: now });
  if (since.current.phrase !== phrase) since.current = { phrase, at: now };
  return Math.max(0, Math.round((now - since.current.at) / 1000));
}
