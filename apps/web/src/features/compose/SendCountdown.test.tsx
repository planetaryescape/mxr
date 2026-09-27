import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { SendCountdownBar, SendCountdownTitle } from "./SendCountdown";

afterEach(() => vi.useRealTimers());

describe("send countdown", () => {
  test("counts whole seconds down from the deadline, then says it is sending", () => {
    vi.useFakeTimers();
    render(<SendCountdownTitle deadline={Date.now() + 5000} />);
    expect(screen.getByTestId("send-countdown")).toHaveTextContent("Sending in 5s");
    act(() => {
      vi.advanceTimersByTime(2100);
    });
    expect(screen.getByTestId("send-countdown")).toHaveTextContent("Sending in 3s");
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByTestId("send-countdown")).toHaveTextContent("Sending…");
  });

  test("the bar drains over exactly the undo window", () => {
    render(<SendCountdownBar seconds={10} />);
    const bar = screen.getByTestId("send-countdown-bar");
    expect(bar).toHaveClass("countdown-drain");
    expect(bar.style.getPropertyValue("--countdown-duration")).toBe("10s");
  });
});
