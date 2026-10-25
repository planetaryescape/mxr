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
    onAccept?: () => void;
    onAgain?: () => void;
    onRetryModel?: (model: string | undefined) => void;
    modelChoices?: components["schemas"]["ModelChoiceData"][];
  } = {},
) {
  const onAccept = vi.fn<() => void>();
  const onAgain = vi.fn<() => void>();
  render(
    <DraftProof
      runs={runs}
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
    />,
  );
  return { onAccept, onAgain };
}

describe("DraftProof keys", () => {
  test("choosing a model retries the shown proof with that model", () => {
    const onRetryModel = vi.fn<(model: string | undefined) => void>();
    renderProof({
      onRetryModel,
      modelChoices: [
        {
          id: "gemma4:latest",
          label: "Gemma 4",
          cloud: false,
          available: true,
          sees: "this conversation and your past mail",
        },
      ],
    });
    fireEvent.change(screen.getByRole("combobox", { name: "Try again with a model" }), {
      target: { value: "gemma4:latest" },
    });
    expect(onRetryModel).toHaveBeenCalledWith("gemma4:latest");
  });

  test("Enter on the proof accepts it", () => {
    const { onAccept } = renderProof();
    fireEvent.keyDown(screen.getByRole("region", { name: "Draft proof" }), { key: "Enter" });
    expect(onAccept).toHaveBeenCalledTimes(1);
  });

  test("Enter on a focused button activates that button, not accept", () => {
    const { onAccept } = renderProof();
    fireEvent.keyDown(screen.getByRole("button", { name: "Keep mine" }), { key: "Enter" });
    expect(onAccept).not.toHaveBeenCalled();
  });

  test("Tab on a focused button is left to move focus, not to cycle gaps", () => {
    renderProof();
    const keepMine = screen.getByRole("button", { name: "Keep mine" });
    const notPrevented = fireEvent.keyDown(keepMine, { key: "Tab" });
    expect(notPrevented).toBe(true);
  });

  test("Tab on the proof moves from gap to gap", () => {
    renderProof();
    const gap = screen.getByText("[[?: a time]]");
    gap.focus();
    const notPrevented = fireEvent.keyDown(gap, { key: "Tab" });
    expect(notPrevented).toBe(false);
  });

  test("a gap the word diff split into three runs is one gap with one Tab stop", () => {
    // "[[?: the date]]" became "[[?: the exact date]]": the diff split the gap
    // into kept, added and kept runs, and all three carry the gap's index.
    const gapRuns: ProofRun[] = [
      { kind: "kept", text: "Meet at ", gap: null },
      { kind: "kept", text: "[[?: the ", gap: 0 },
      { kind: "added", text: "exact ", gap: 0 },
      { kind: "kept", text: "date]]", gap: 0 },
      { kind: "kept", text: " tomorrow", gap: null },
    ];
    render(
      <DraftProof
        runs={gapRuns}
        gapCount={1}
        provenance={null}
        versionIndex={0}
        versionCount={1}
        stale={false}
        onAccept={vi.fn<() => void>()}
        onKeep={vi.fn<() => void>()}
        onStep={vi.fn<(delta: number) => void>()}
        onAgain={vi.fn<() => void>()}
        onRetryModel={vi.fn<(model: string | undefined) => void>()}
        modelChoices={[]}
        defaultModel="gemma4:latest"
        generating={false}
        onRevise={vi.fn<() => void>()}
      />,
    );
    const gaps = document.querySelectorAll("[data-gap]");
    expect(gaps).toHaveLength(1);
    expect(gaps[0]?.textContent).toBe("[[?: the exact date]]");
    expect(screen.getByText(/1 gap left/)).toBeInTheDocument();
  });
});
