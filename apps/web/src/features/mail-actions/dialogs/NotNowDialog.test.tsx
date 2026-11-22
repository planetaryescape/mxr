/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { messageNotNow } from "../notNow";
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
          payload: { untilLabel: expect.stringMatching(/\d/) },
        }),
      ),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
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
});
