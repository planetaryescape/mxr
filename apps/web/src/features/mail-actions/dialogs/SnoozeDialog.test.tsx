/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SnoozeDialog } from "./SnoozeDialog";

const api = vi.hoisted(() => ({
  fetchSnoozePresets: vi.fn<() => Promise<unknown>>(),
  snoozeMessages: vi.fn<(ids: string[], until: string) => Promise<unknown[]>>(),
}));

vi.mock("./api", () => ({
  fetchSnoozePresets: api.fetchSnoozePresets,
  shellKey: ["shell"],
  snoozeMessages: api.snoozeMessages,
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
    api.snoozeMessages.mockResolvedValue([]);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("snoozes selected messages with a bridge preset", async () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-1"]} onOpenChange={onOpenChange} />);

    fireEvent.click(await screen.findByRole("button", { name: /tomorrow morning/i }));

    await waitFor(() => expect(api.snoozeMessages).toHaveBeenCalledWith(["msg-1"], "tomorrow"));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  test("snoozes selected messages with a custom natural-language time", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-2"]} onOpenChange={() => {}} />);

    fireEvent.change(await screen.findByLabelText(/custom snooze time/i), {
      target: { value: "in 2h" },
    });
    fireEvent.click(screen.getByRole("button", { name: /snooze custom time/i }));

    await waitFor(() => expect(api.snoozeMessages).toHaveBeenCalledWith(["msg-2"], "in 2h"));
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

    expect(await screen.findByRole("button", { name: /tomorrow morning/i })).toBeVisible();
    expect(screen.queryByRole("button", { name: /tonight/i })).not.toBeInTheDocument();
  });
});

function tomorrowAt(hour: number): Date {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  date.setHours(hour, 0, 0, 0);
  return date;
}
