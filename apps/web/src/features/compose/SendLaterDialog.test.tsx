/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { SendLaterDialog } from "./SendLaterDialog";

describe("SendLaterDialog", () => {
  test("previews a parsed natural-language time and confirms with it", () => {
    const onConfirm = vi.fn<(at: Date, label: string) => void>();
    render(
      <SendLaterDialog open scheduling={false} onOpenChange={() => {}} onConfirm={onConfirm} />,
    );

    const input = screen.getByLabelText("Custom send time");
    fireEvent.change(input, { target: { value: "in 2 hours" } });

    expect(screen.getByRole("status")).toHaveTextContent(/^Sends /);

    fireEvent.click(screen.getByRole("button", { name: "Schedule send" }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    const [at, label] = onConfirm.mock.calls[0] ?? [];
    expect(at).toBeInstanceOf(Date);
    expect((at as Date).getTime()).toBeGreaterThan(Date.now());
    expect(label).toBeTruthy();
  });

  test("keeps confirm disabled for unparseable input", () => {
    render(
      <SendLaterDialog open scheduling={false} onOpenChange={() => {}} onConfirm={() => {}} />,
    );

    const input = screen.getByLabelText("Custom send time");
    fireEvent.change(input, { target: { value: "whenever you fancy" } });

    expect(screen.getByRole("button", { name: "Schedule send" })).toBeDisabled();
  });

  test("presets confirm without typing", () => {
    const onConfirm = vi.fn<(at: Date, label: string) => void>();
    render(
      <SendLaterDialog open scheduling={false} onOpenChange={() => {}} onConfirm={onConfirm} />,
    );

    fireEvent.click(screen.getByRole("button", { name: /tomorrow 9am/i }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    const [at] = onConfirm.mock.calls[0] ?? [];
    expect((at as Date).getHours()).toBe(9);
  });
});
