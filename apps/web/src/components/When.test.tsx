import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { When } from "./When";

describe("When", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 1, 15, 0, 0));
  });
  afterEach(() => vi.useRealTimers());

  test("a date's words move on with the clock, and only the date re-renders", () => {
    let parentRenders = 0;
    function Row() {
      parentRenders += 1;
      return (
        <p data-testid="row">
          Maya <When value={new Date(2026, 9, 1, 14, 59, 50)} />
        </p>
      );
    }
    render(<Row />);
    expect(screen.getByTestId("row")).toHaveTextContent("Maya Just now");

    act(() => vi.advanceTimersByTime(90_000));
    expect(screen.getByTestId("row")).toHaveTextContent("Maya 1m");

    act(() => vi.advanceTimersByTime(30_000));
    expect(screen.getByTestId("row")).toHaveTextContent("Maya 2m");
    expect(parentRenders).toBe(1);
  });
});
