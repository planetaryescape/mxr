/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SnoozeDialog } from "./SnoozeDialog";

const api = vi.hoisted(() => ({
  fetchSnoozePresets: vi.fn<() => Promise<unknown>>(),
  performMailAction:
    vi.fn<(action: string, ids: string[], options?: { until?: string }) => Promise<unknown>>(),
}));

vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  fetchSnoozePresets: api.fetchSnoozePresets,
}));

vi.mock("@/features/time/api", async (importOriginal) => {
  const { fakeResolvedTime } = await import("@/features/time/testing");
  return {
    ...(await importOriginal<typeof import("@/features/time/api")>()),
    resolveTime: (input: string) => Promise.resolve(fakeResolvedTime(input)),
  };
});

vi.mock("../mailMutations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../mailMutations")>()),
  performMailAction: api.performMailAction,
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("SnoozeDialog", () => {
  beforeEach(() => {
    api.fetchSnoozePresets.mockResolvedValue({
      presets: [
        {
          id: "tomorrow",
          label: "Tomorrow morning",
          wakeAt: "2026-05-12T09:00:00Z",
        },
      ],
    });
    api.performMailAction.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("a preset stores the wake time it showed", async () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-1"]} onOpenChange={onOpenChange} />);

    fireEvent.click(await screen.findByText("Tomorrow morning"));

    await waitFor(() =>
      expect(api.performMailAction).toHaveBeenCalledWith("snooze", ["msg-1"], {
        until: "2026-05-12T09:00:00Z",
      }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  test("number keys pick a preset", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-4"]} onOpenChange={() => {}} />);
    await screen.findByText("Tomorrow morning");

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "1" });

    expect(api.performMailAction).toHaveBeenCalledWith("snooze", ["msg-4"], {
      until: "2026-05-12T09:00:00Z",
    });
  });

  test("a typed time stores the instant the preview showed", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-2"]} onOpenChange={() => {}} />);

    fireEvent.change(await screen.findByLabelText(/or type a time/i), {
      target: { value: "in 2h" },
    });
    await screen.findByText(/in 2 hours/);
    fireEvent.click(screen.getByRole("button", { name: /^snooze$/i }));

    await waitFor(() => expect(api.performMailAction).toHaveBeenCalledTimes(1));
    const [, ids, options] = api.performMailAction.mock.calls[0] ?? [];
    expect(ids).toEqual(["msg-2"]);
    // An RFC3339 instant, not the words, so the daemon can't re-resolve it.
    expect(new Date(options?.until ?? "").getTime() - Date.now()).toBeGreaterThan(7_000_000);
  });

  test("an ambiguous time offers both readings and snoozes to the one picked", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-5"]} onOpenChange={() => {}} />);

    const field = await screen.findByLabelText(/or type a time/i);
    fireEvent.change(field, { target: { value: "fri 3" } });

    const afternoon = await screen.findByRole("radio", { name: /15:00/ });
    const early = screen.getByRole("radio", { name: /03:00/ });
    expect(afternoon).toHaveAttribute("aria-checked", "true");

    fireEvent.keyDown(field, { key: "ArrowDown" });
    expect(early).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(field, { key: "Enter" });

    await waitFor(() => expect(api.performMailAction).toHaveBeenCalledTimes(1));
    const until = api.performMailAction.mock.calls[0]?.[2]?.until ?? "";
    expect(new Date(until).getHours()).toBe(3);
  });

  test("a phrase it can't read says so and can't be submitted", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-6"]} onOpenChange={() => {}} />);

    fireEvent.change(await screen.findByLabelText(/or type a time/i), {
      target: { value: "frday" },
    });

    expect(await screen.findByText('Didn\'t catch "frday". Try "fri 3pm" or "in 2d".')).toBeVisible();
    expect(screen.getByRole("button", { name: /^snooze$/i })).toBeDisabled();
  });

  test("hides the tonight preset when it resolves to tomorrow", async () => {
    const tomorrowMorning = tomorrowAt(9);
    const tomorrowEvening = tomorrowAt(18);
    api.fetchSnoozePresets.mockResolvedValue({
      presets: [
        {
          id: "tomorrow",
          label: "Tomorrow morning",
          wakeAt: tomorrowMorning.toISOString(),
        },
        {
          id: "tonight",
          label: "Tonight",
          wakeAt: tomorrowEvening.toISOString(),
        },
      ],
    });

    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-3"]} onOpenChange={() => {}} />);

    expect(await screen.findByText("Tomorrow morning")).toBeVisible();
    expect(screen.queryByText("Tonight")).not.toBeInTheDocument();
  });
});

function tomorrowAt(hour: number): Date {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  date.setHours(hour, 0, 0, 0);
  return date;
}
