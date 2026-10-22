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
import type { ProofPiece } from "./assist/proof";
import type { DraftProvenance } from "./draftProvenanceFormat";

const PIECE_CLASS: Record<ProofPiece["kind"], string> = {
  kept: "text-foreground",
  added: "text-foreground underline decoration-signal decoration-1 underline-offset-4",
  removed: "text-text-3 line-through decoration-1",
  gap: "text-cue",
};

interface DraftProofProps {
  pieces: ProofPiece[];
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
  pieces,
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
  const gapCount = pieces.filter((piece) => piece.kind === "gap").length;
  // Each run starts at a distinct offset, so the offset keys it without an index.
  let offset = 0;
  const keyed = pieces.map((piece) => {
    const key = `${offset}:${piece.kind}`;
    offset += piece.text.length;
    return { key, piece };
  });

  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  function onKeyDown(event: KeyboardEvent<HTMLElement>) {
    if (isComposingKey(event)) return;
    if (event.key === "Enter" && !event.shiftKey && !event.metaKey && !event.ctrlKey) {
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
    } else if (event.key === "Tab" && gapCount > 0) {
      // Tab moves gap to gap; with no gap it is ordinary focus movement.
      const gaps = Array.from(
        containerRef.current?.querySelectorAll<HTMLElement>("[data-gap]") ?? [],
      );
      const current = gaps.indexOf(document.activeElement as HTMLElement);
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
            {keyed.map(({ key, piece }) =>
              piece.kind === "removed" ? (
                <span key={key} className={PIECE_CLASS.removed}>
                  {piece.text}
                </span>
              ) : piece.kind === "gap" ? (
                <span key={key} data-gap tabIndex={0} className={PIECE_CLASS.gap}>
                  {piece.text}
                </span>
              ) : (
                <span key={key} className={cn(PIECE_CLASS[piece.kind])}>
                  {piece.text}
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
