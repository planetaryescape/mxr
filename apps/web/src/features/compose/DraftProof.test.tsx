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
  test("shows style marks on their text with the pattern and context on hover", () => {
    renderProof({
      styleMarks: [
        { name: "Not X but Y", context: "This is not speed but cost.", start: 0, end: 8 },
      ],
    });
    expect(screen.getByText("Thursday", { exact: false }).closest("span")).toHaveAttribute(
      "title",
      "Not X but Y: This is not speed but cost.",
    );
    expect(screen.getByText(/Style 92\/100 · 1 mark/)).toBeInTheDocument();
  });
  test("keeps UTF-8 marks aligned after removed text and combines overlaps", () => {
    renderProof({
      runs: [
        { kind: "removed", text: "old", gap: null },
        { kind: "kept", text: "Café ", gap: null },
        { kind: "added", text: "résumé", gap: null },
      ],
      styleMarks: [
        { name: "First", context: "Café", start: 0, end: 6 },
        { name: "Second", context: "Café résumé", start: 3, end: 10 },
      ],
    });
    expect(screen.getByRole("region", { name: "Draft proof" }).textContent).toContain(
      "Café résumé",
    );
    const overlap = Array.from(document.querySelectorAll<HTMLElement>("[title]")).find(
      (element) => element.title.includes("First:") && element.title.includes("Second:"),
    );
    expect(overlap?.textContent).toBe("é ");
  });
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
