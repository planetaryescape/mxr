import type { ComposeAssistChange, ComposeAssistGap } from "./types";

export type ProofRunKind = "kept" | "added" | "removed";

export interface ProofRun {
  kind: ProofRunKind;
  text: string;
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

export function runsText(runs: readonly ProofRun[]): string {
  return runs
    .filter((run) => run.kind !== "removed")
    .map((run) => run.text)
    .join("");
}
