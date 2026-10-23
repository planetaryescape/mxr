/*
 * The proof replaces the page while it is open. The user's words are in
 * plain ink, additions are underlined in the signal colour, cuts are struck
 * through in text-3, and gaps are in the cue colour. Nothing is written
 * into the draft until Accept.
 */

import { useEffect, useRef, type KeyboardEvent } from "react";

import { Button } from "@/components/ui/button";
import { isComposingKey } from "@/lib/keys/composition";
import { cn } from "@/lib/utils";
import { DraftProvenanceLine } from "./DraftProvenanceLine";
import type { ProofRun, ProofRunKind } from "./assist/proof";
import type { DraftProvenance } from "./draftProvenanceFormat";

/** How a run is drawn outside a gap. */
const RUN_CLASS = {
  kept: "text-foreground",
  added: "text-foreground underline decoration-signal decoration-1 underline-offset-4",
  removed: "text-text-3 line-through decoration-1",
} satisfies Record<ProofRunKind, string>;

interface DraftProofProps {
  runs: ProofRun[];
  /** One Tab stop and one count per gap in the proposal, however the diff split it. */
  gapCount: number;
  provenance: DraftProvenance | null;
  versionIndex: number;
  versionCount: number;
  /** The text changed since the proof was made, so it cannot be applied. */
  stale: boolean;
  onAccept: () => void;
  onKeep: () => void;
  onStep: (delta: number) => void;
  onAgain: () => void;
  onRevise: () => void;
}

export function DraftProof({
  runs,
  gapCount,
  provenance,
  versionIndex,
  versionCount,
  stale,
  onAccept,
  onKeep,
  onStep,
  onAgain,
  onRevise,
}: DraftProofProps) {
  const containerRef = useRef<HTMLElement>(null);
  const groups = groupRuns(runs);

  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  function onKeyDown(event: KeyboardEvent<HTMLElement>) {
    if (isComposingKey(event)) return;
    // Enter and Tab belong to the proof only while it or one of its gaps has
    // focus; on a focused button they keep their own meaning (activate, move).
    const target = event.target;
    const onProofSurface =
      target === containerRef.current ||
      (target instanceof HTMLElement && target.closest("[data-gap]") !== null);
    if (event.key === "Enter" && !event.shiftKey && !event.metaKey && !event.ctrlKey) {
      if (!onProofSurface) return;
      event.preventDefault();
      if (stale) onAgain();
      else onAccept();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onKeep();
    } else if (event.key === "ArrowLeft" && versionCount > 1) {
      event.preventDefault();
      onStep(-1);
    } else if (event.key === "ArrowRight" && versionCount > 1) {
      event.preventDefault();
      onStep(1);
    } else if (event.key === "Tab" && gapCount > 0 && onProofSurface) {
      // Tab moves gap to gap; with no gap it is ordinary focus movement.
      const gaps = Array.from(
        containerRef.current?.querySelectorAll<HTMLElement>("[data-gap]") ?? [],
      );
      const focused = document.activeElement;
      const current = gaps.findIndex((gap) => gap === focused || gap.contains(focused));
      const next =
        current === -1
          ? gaps[event.shiftKey ? gaps.length - 1 : 0]
          : gaps[(current + (event.shiftKey ? -1 : 1) + gaps.length) % gaps.length];
      event.preventDefault();
      next?.focus();
    }
  }

  return (
    <section
      ref={containerRef}
      tabIndex={-1}
      aria-label="Draft proof"
      onKeyDown={onKeyDown}
      className="flex h-full min-h-0 flex-col overflow-hidden outline-none"
    >
      <div className="min-h-0 flex-1 overflow-auto">
        <div className="mx-auto w-full max-w-[860px] px-5 py-5">
          <div className="type-reading whitespace-pre-wrap border-l-2 border-signal pl-4">
            {groups.map((group) =>
              group.gap === null ? (
                group.items.map((item) => <RunText key={item.key} run={item.run} />)
              ) : (
                // One focusable gap, however many runs the word diff put inside it.
                <span key={group.key} data-gap tabIndex={0} className="text-cue">
                  {group.items.map((item) => (
                    <RunText key={item.key} run={item.run} inGap />
                  ))}
                </span>
              ),
            )}
          </div>
          <div className="mt-5 flex flex-wrap items-center gap-2">
            {stale ? (
              <Button type="button" onClick={onAgain}>
                Run again on your new text
              </Button>
            ) : (
              <Button type="button" onClick={onAccept}>
                Accept
              </Button>
            )}
            <Button type="button" variant="outline" onClick={onAgain} disabled={stale}>
              Try again
            </Button>
            <Button type="button" variant="ghost" onClick={onRevise}>
              Revise
            </Button>
            <Button type="button" variant="ghost" onClick={onKeep}>
              Keep mine
            </Button>
          </div>
          <div className="type-meta mt-3 space-y-1 text-text-3">
            <DraftProvenanceLine provenance={provenance} />
            <p>
              {versionCount > 1 ? `Version ${versionIndex + 1} of ${versionCount} · ← → ` : ""}
              {gapCount > 0
                ? `${gapCount} gap${gapCount === 1 ? "" : "s"} left · Tab moves between them`
                : ""}
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

interface RunItem {
  key: string;
  run: ProofRun;
}

interface RunGroup {
  key: string;
  gap: number | null;
  items: RunItem[];
}

/**
 * Runs in order, with the runs of one gap kept together. Each run starts at a
 * distinct offset, so the offset keys it without an index.
 */
function groupRuns(runs: readonly ProofRun[]): RunGroup[] {
  const groups: RunGroup[] = [];
  let offset = 0;
  for (const run of runs) {
    const item: RunItem = { key: `${offset}:${run.kind}`, run };
    offset += run.text.length;
    const last = groups.at(-1);
    if (run.gap !== null && last?.gap === run.gap) {
      last.items.push(item);
    } else {
      groups.push({
        key: run.gap === null ? item.key : `gap-${run.gap}`,
        gap: run.gap,
        items: [item],
      });
    }
  }
  return groups;
}

function RunText({ run, inGap = false }: { run: ProofRun; inGap?: boolean }) {
  // Inside a gap the colour is the gap's; the run keeps its own addition or strike.
  const className = inGap ? cn("text-cue", decorationOnly(run.kind)) : RUN_CLASS[run.kind];
  return <span className={className}>{run.text}</span>;
}

function decorationOnly(kind: ProofRunKind): string {
  if (kind === "added") return "underline decoration-signal decoration-1 underline-offset-4";
  if (kind === "removed") return "line-through decoration-1";
  return "";
}
