/* @vitest-environment jsdom */

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { MessageBody } from "./MessageBody";

describe("MessageBody resize timers", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    cleanup();
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it("replaces pending measurements on source changes and repeated loads, then clears them on unmount", () => {
    const view = render(<MessageBody html="<p>first</p>" />);
    const frame = screen.getByTitle("HTML message body");

    fireEvent.load(frame);
    expect(vi.getTimerCount()).toBe(2);

    view.rerender(<MessageBody html="<p>second</p>" />);
    expect(vi.getTimerCount()).toBe(0);

    fireEvent.load(frame);
    fireEvent.load(frame);
    expect(vi.getTimerCount()).toBe(2);

    act(() => vi.advanceTimersByTime(100));
    expect(vi.getTimerCount()).toBe(1);
    act(() => vi.advanceTimersByTime(400));
    expect(vi.getTimerCount()).toBe(0);

    fireEvent.load(frame);
    expect(vi.getTimerCount()).toBe(2);

    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
