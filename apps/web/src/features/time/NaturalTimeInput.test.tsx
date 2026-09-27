/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { ResolvedTime, TimeChoice } from "./api";
import { NaturalTimeInput } from "./NaturalTimeInput";
import { fakeResolvedTime } from "./testing";
import { useNaturalTime } from "./useNaturalTime";

const resolver = vi.hoisted(() => ({
  resolveTime: vi.fn<(input: string, signal?: AbortSignal) => Promise<ResolvedTime>>(),
}));

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  resolveTime: resolver.resolveTime,
}));

function Field({ onCommit }: { onCommit: (choice: TimeChoice) => void }) {
  const state = useNaturalTime();
  return (
    <>
      <label htmlFor="when">When</label>
      <NaturalTimeInput id="when" state={state} onCommit={onCommit} />
    </>
  );
}

function renderField(onCommit: (choice: TimeChoice) => void = () => {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <Field onCommit={onCommit} />
    </QueryClientProvider>,
  );
  return screen.getByLabelText("When");
}

afterEach(() => {
  vi.clearAllMocks();
});

describe("NaturalTimeInput", () => {
  test("highlights the understood text and mutes the assumed part", async () => {
    resolver.resolveTime.mockImplementation((input) => Promise.resolve(fakeResolvedTime(input)));
    const field = renderField();

    fireEvent.change(field, { target: { value: "fri 3" } });

    await waitFor(() => expect(document.querySelector("mark[data-understood]")).not.toBeNull());
    expect(document.querySelector("mark[data-understood]")).toHaveTextContent("fri 3");
    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent(/15:00/);
    // "fri 3" leaves out am or pm, so the time is shown as assumed.
    expect(status.querySelector("[data-assumed]")).toHaveTextContent("15:00");
  });

  test("chips pick a reading by arrow keys, number keys and click; Enter commits it", async () => {
    resolver.resolveTime.mockImplementation((input) => Promise.resolve(fakeResolvedTime(input)));
    const onCommit = vi.fn<(choice: TimeChoice) => void>();
    const field = renderField(onCommit);

    fireEvent.change(field, { target: { value: "fri 3" } });
    const group = await screen.findByRole("radiogroup", { name: "Which time did you mean?" });
    const [afternoon, early] = screen.getAllByRole("radio");
    expect(afternoon).toHaveAttribute("aria-checked", "true");

    fireEvent.keyDown(group, { key: "ArrowRight" });
    expect(early).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(group, { key: "1" });
    expect(afternoon).toHaveAttribute("aria-checked", "true");
    fireEvent.click(early!);
    expect(early).toHaveAttribute("aria-checked", "true");

    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    expect(onCommit.mock.calls[0]?.[0].label).toBe("03:00");
  });

  test("a slow answer for older text never shows as the answer for newer text", async () => {
    const pending = new Map<string, (value: ResolvedTime) => void>();
    resolver.resolveTime.mockImplementation(
      (input) => new Promise((resolve) => pending.set(input, resolve)),
    );
    const onCommit = vi.fn<(choice: TimeChoice) => void>();
    const field = renderField(onCommit);

    fireEvent.change(field, { target: { value: "in 3 days" } });
    await waitFor(() => expect(pending.has("in 3 days")).toBe(true));
    fireEvent.change(field, { target: { value: "in 2 hours" } });
    await waitFor(() => expect(pending.has("in 2 hours")).toBe(true));

    // The newer answer lands first, then the stale one.
    await act(async () => pending.get("in 2 hours")?.(fakeResolvedTime("in 2 hours")));
    await act(async () => pending.get("in 3 days")?.(fakeResolvedTime("in 3 days")));

    expect(screen.getByRole("status")).toHaveTextContent(/in 2 hours$/);
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    const at = new Date(onCommit.mock.calls[0]?.[0].at ?? "").getTime();
    expect(at - Date.now()).toBeLessThan(3 * 3_600_000);
  });

  test("an unreadable phrase shows the daemon's message and nothing commits", async () => {
    resolver.resolveTime.mockImplementation((input) => Promise.resolve(fakeResolvedTime(input)));
    const onCommit = vi.fn<(choice: TimeChoice) => void>();
    const field = renderField(onCommit);

    fireEvent.change(field, { target: { value: "frday" } });
    expect(
      await screen.findByText('Didn\'t catch "frday". Try "fri 3pm" or "in 2d".'),
    ).toBeVisible();
    expect(field).toHaveAttribute("aria-invalid", "true");

    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(resolver.resolveTime).toHaveBeenCalled());
    expect(onCommit).not.toHaveBeenCalled();
  });

  test("a bridge failure says so calmly", async () => {
    resolver.resolveTime.mockRejectedValue(new Error("connection refused"));
    const field = renderField();

    fireEvent.change(field, { target: { value: "tomorrow" } });

    expect(await screen.findByText("Couldn't check that time: connection refused")).toBeVisible();
  });
});
