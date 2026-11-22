import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { components } from "@/api/generated";
import { DraftProof } from "./DraftProof";
import type { ProofRun } from "./assist/proof";

const runs: ProofRun[] = [
  { kind: "kept", text: "Thursday works. ", gap: null },
  { kind: "kept", text: "[[?: a time]]", gap: 0 },
];

function renderProof(
  overrides: {
    runs?: ProofRun[];
    onAccept?: () => void;
    onAgain?: () => void;
    onRetryModel?: (model: string | undefined) => void;
    modelChoices?: components["schemas"]["ModelChoiceData"][];
    styleMarks?: { name: string; context: string; start: number; end: number }[];
  } = {},
) {
  const onAccept = vi.fn<() => void>();
  const onAgain = vi.fn<() => void>();
  render(
    <DraftProof
      runs={overrides.runs ?? runs}
      gapCount={1}
      provenance={null}
      versionIndex={0}
      versionCount={1}
      stale={false}
      onAccept={overrides.onAccept ?? onAccept}
      onKeep={vi.fn<() => void>()}
      onStep={vi.fn<(delta: number) => void>()}
      onAgain={overrides.onAgain ?? onAgain}
      onRetryModel={overrides.onRetryModel ?? vi.fn<(model: string | undefined) => void>()}
      modelChoices={overrides.modelChoices ?? []}
      defaultModel="gemma4:latest"
      generating={false}
      onRevise={vi.fn<() => void>()}
      styleMarks={overrides.styleMarks}
      styleScore={overrides.styleMarks ? 92 : undefined}
    />,
  );
  return { onAccept, onAgain };
}

describe("DraftProof keys", () => {
  test("Enter on the proof accepts it", () => {
    const { onAccept } = renderProof();
    fireEvent.keyDown(screen.getByRole("region", { name: "Draft proof" }), { key: "Enter" });
    expect(onAccept).toHaveBeenCalledTimes(1);
  });
});
