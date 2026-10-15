import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { AnchoredHint } from "./AnchoredHint";
import type { Hint } from "./api";

const hint: Hint = {
  id: "updates.suggestion",
  text: "Nothing becomes a to-do on its own.",
  seen: false,
  anchor: "updates",
  key: { key: "t", verb: "add to To do" },
};

describe("a hint", () => {
  test("is announced as a status with its text", () => {
    render(<AnchoredHint hint={hint} onDismiss={() => {}} />);
    expect(screen.getByRole("status")).toHaveTextContent("Nothing becomes a to-do on its own.");
  });

  test("keeps its dismiss control in the tab order, and a click dismisses it", () => {
    const onDismiss = vi.fn<() => void>();
    render(<AnchoredHint hint={hint} onDismiss={onDismiss} />);
    const dismiss = screen.getByRole("button", { name: "Dismiss hint (Esc)" });
    expect(dismiss.tabIndex).toBeGreaterThanOrEqual(0);
    fireEvent.click(dismiss);
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
