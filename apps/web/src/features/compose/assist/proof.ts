/*
 * A proof as the page draws it: the daemon's change list cut into runs that
 * say what each run of words is. Kept words are the user's own; added words
 * came from the model; removed words are struck. Gaps are the `[[?: …]]`
 * spots the user still has to fill, and their identity comes from the
 * proposal's byte ranges, not from the runs: a word diff can split one gap
 * across several runs. Gap offsets are UTF-8 bytes into the proposed text, like
 * every other offset on this contract.
 */

import type { ComposeAssistChange, ComposeAssistGap } from "./types";

export type ProofRunKind = "kept" | "added" | "removed";

export interface ProofRun {
  kind: ProofRunKind;
  text: string;
  /** Index into `proposal.gaps` when the run lies inside a gap, else null. */
  gap: number | null;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function proofRuns(
  changes: readonly ComposeAssistChange[],
  gaps: readonly ComposeAssistGap[],
): ProofRun[] {
  const runs: ProofRun[] = [];
  let offset = 0;
  for (const change of changes) {
    if (change.kind === "removed") {
      if (change.text) runs.push({ kind: "removed", text: change.text, gap: null });
      continue;
    }
    const bytes = encoder.encode(change.text);
    const start = offset;
    const end = offset + bytes.length;
    offset = end;

    // Cut the run at every gap edge, so each part is wholly inside a gap or out.
    const cuts = new Set<number>([start, end]);
    for (const gap of gaps) {
      if (gap.start > start && gap.start < end) cuts.add(gap.start);
      if (gap.end > start && gap.end < end) cuts.add(gap.end);
    }
    const sorted = [...cuts].toSorted((a, b) => a - b);
    for (let index = 0; index < sorted.length - 1; index++) {
      const from = sorted[index] ?? start;
      const to = sorted[index + 1] ?? end;
      const text = decoder.decode(bytes.slice(from - start, to - start));
      if (text === "") continue;
      const gapIndex = gaps.findIndex((gap) => gap.start <= from && to <= gap.end);
      runs.push({
        kind: change.kind === "added" ? "added" : "kept",
        text,
        gap: gapIndex === -1 ? null : gapIndex,
      });
    }
  }
  return attachRemovedToGaps(runs);
}

/**
 * A removed run has no place in the proposed text. It belongs to a gap when
 * the gap's runs on both sides of it are the same gap, so the struck words
 * sit inside the gap's span rather than splitting it.
 */
function attachRemovedToGaps(runs: ProofRun[]): ProofRun[] {
  return runs.map((run, index) => {
    if (run.kind !== "removed") return run;
    const before = runs.slice(0, index).findLast((other) => other.kind !== "removed");
    const after = runs.slice(index + 1).find((other) => other.kind !== "removed");
    return before && after && before.gap !== null && before.gap === after.gap
      ? { ...run, gap: before.gap }
      : run;
  });
}

/** The proposed text the runs spell, which must equal `proposal.text`. */
export function runsText(runs: readonly ProofRun[]): string {
  return runs
    .filter((run) => run.kind !== "removed")
    .map((run) => run.text)
    .join("");
}
