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
});

describe("NaturalTimeInput fast Enter", () => {
  test("repeated Enter presses while a commit is in flight submit once", async () => {
    resolver.resolveTime.mockImplementation((input) => Promise.resolve(fakeResolvedTime(input)));
    let finish: (() => void) | undefined;
    const onCommit = vi.fn<(choice: TimeChoice) => Promise<void>>(
      () => new Promise<void>((resolve) => (finish = resolve)),
    );
    const field = renderField(onCommit);

    fireEvent.change(field, { target: { value: "in 2 hours" } });
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/in 2 hours$/));
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));

    await act(async () => finish?.());
    expect(onCommit).toHaveBeenCalledTimes(1);
  });
});
