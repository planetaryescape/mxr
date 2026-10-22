import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { DraftProof } from "./DraftProof";
import type { ProofPiece } from "./assist/proof";

const pieces: ProofPiece[] = [
  { kind: "kept", text: "Thursday works. " },
  { kind: "gap", text: "[[?: a time]]" },
];

function renderProof(overrides: { onAccept?: () => void; onAgain?: () => void } = {}) {
  const onAccept = vi.fn<() => void>();
  const onAgain = vi.fn<() => void>();
  render(
    <DraftProof
      pieces={pieces}
      provenance={null}
      versionIndex={0}
      versionCount={1}
      stale={false}
      onAccept={overrides.onAccept ?? onAccept}
      onKeep={vi.fn<() => void>()}
      onStep={vi.fn<(delta: number) => void>()}
      onAgain={overrides.onAgain ?? onAgain}
      onRevise={vi.fn<() => void>()}
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
});
