/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { messageNotNow, type NotNowPicker } from "../notNow";
import { NotNowDialog } from "./NotNowDialog";

const api = vi.hoisted(() => ({
  fetchSnoozePresets: vi.fn<() => Promise<unknown>>(),
  setReplyLater: vi.fn<(messageId: string, flag: boolean) => Promise<unknown>>(),
  performMailAction:
    vi.fn<
      (
        action: string,
        ids: string[],
        options?: { until?: string; payload?: { untilLabel?: string } },
      ) => Promise<unknown>
    >(),
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
  setReplyLater: vi.fn<(messageId: string, flag: boolean) => Promise<unknown>>(),
  performMailAction: api.performMailAction,
}));

vi.mock("@/features/reply-queue/api", () => ({ setReplyLater: api.setReplyLater }));

/** The mail Not-now picker for these messages, as `b` on a list row opens it. */
function SnoozeDialog({
  messageIds,
  onOpenChange,
  replyLaterId,
}: {
  open?: boolean;
  messageIds: string[];
  onOpenChange: (open: boolean) => void;
  replyLaterId?: string;
}) {
  return (
    <NotNowDialog
      picker={messageNotNow({ messageIds, replyLaterId, subject: "Q4 plan" })}
      onClose={() => onOpenChange(false)}
    />
  );
}

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("NotNowDialog", () => {
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
    api.setReplyLater.mockResolvedValue({});
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("a preset stores the wake time it showed", async () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-1"]} onOpenChange={onOpenChange} />);

    fireEvent.click(await screen.findByText("Tomorrow morning"));

    await waitFor(() =>
      expect(api.performMailAction).toHaveBeenCalledWith(
        "snooze",
        ["msg-1"],
        expect.objectContaining({
          until: "2026-05-12T09:00:00Z",
          // The toast names the time the row showed.
          payload: { untilLabel: expect.stringMatching(/\d/) },
        }),
      ),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  test("number keys pick a preset", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-4"]} onOpenChange={() => {}} />);
    await screen.findByText("Tomorrow morning");

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "1" });

    expect(api.performMailAction).toHaveBeenCalledWith(
      "snooze",
      ["msg-4"],
      expect.objectContaining({ until: "2026-05-12T09:00:00Z" }),
    );
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
    const options = api.performMailAction.mock.calls[0]?.[2];
    expect(new Date(options?.until ?? "").getHours()).toBe(3);
    expect(options?.payload?.untilLabel).toMatch(/, 03:00$/);
  });

  test("a phrase it can't read says so and can't be submitted", async () => {
    renderWithQueryClient(<SnoozeDialog open messageIds={["msg-6"]} onOpenChange={() => {}} />);

    fireEvent.change(await screen.findByLabelText(/or type a time/i), {
      target: { value: "frday" },
    });

    expect(
      await screen.findByText('Didn\'t catch "frday". Try "fri 3pm" or "in 2d".'),
    ).toBeVisible();
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

  test("Later comes first and starts lit, so Enter is the untimed reply later", async () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    renderWithQueryClient(
      <SnoozeDialog messageIds={["msg-7"]} replyLaterId="msg-7" onOpenChange={onOpenChange} />,
    );
    const later = screen.getByTestId("not-now-later");
    await waitFor(() => expect(later).toHaveFocus());
    // Enter on a focused button clicks it.
    fireEvent.click(later);
    await waitFor(() => expect(api.setReplyLater).toHaveBeenCalledWith("msg-7", true));
    expect(api.performMailAction).not.toHaveBeenCalled();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  test("the arrows move the highlight from Later to the presets", async () => {
    renderWithQueryClient(
      <SnoozeDialog messageIds={["msg-8"]} replyLaterId="msg-8" onOpenChange={() => {}} />,
    );
    const preset = (await screen.findByText("Tomorrow morning")).closest("button");
    const later = screen.getByTestId("not-now-later");
    await waitFor(() => expect(later).toHaveFocus());
    fireEvent.keyDown(later, { key: "ArrowDown" });
    expect(preset).toHaveFocus();
    fireEvent.keyDown(preset!, { key: "ArrowUp" });
    expect(later).toHaveFocus();
  });

  test("with no untimed choice there is no Later, and the first preset is lit", async () => {
    const at = vi.fn<NotNowPicker["at"]>();
    renderWithQueryClient(
      <NotNowDialog
        picker={{
          title: "Schedule",
          description: "Pay rent shows up then.",
          timeVerb: "Schedule",
          at,
        }}
        onClose={() => {}}
      />,
    );
    const preset = (await screen.findByText("Tomorrow morning")).closest("button");
    expect(screen.queryByTestId("not-now-later")).toBeNull();
    await waitFor(() => expect(preset).toHaveFocus());
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "1" });
    await waitFor(() =>
      expect(at).toHaveBeenCalledWith("2026-05-12T09:00:00Z", expect.stringMatching(/\d/)),
    );
  });
});

function tomorrowAt(hour: number): Date {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  date.setHours(hour, 0, 0, 0);
  return date;
}
