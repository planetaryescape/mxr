/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { TimeChoice } from "@/features/time/api";

import type { MailTarget } from "../target";
import { ReplyLaterDialog } from "./ReplyLaterDialog";

const api = vi.hoisted(() => ({
  deferThreads: vi.fn<(threadIds: string[], choice: TimeChoice) => Promise<boolean>>(),
  setReplyLater: vi.fn<(messageId: string, flag: boolean) => Promise<unknown>>(),
}));

vi.mock("@/features/desk/deskLater", () => ({ deferThreads: api.deferThreads }));
vi.mock("@/features/reply-queue/api", () => ({ setReplyLater: api.setReplyLater }));
vi.mock("@/features/time/api", async (importOriginal) => {
  const { fakeResolvedTime } = await import("@/features/time/testing");
  return {
    ...(await importOriginal<typeof import("@/features/time/api")>()),
    resolveTime: (input: string) => Promise.resolve(fakeResolvedTime(input)),
  };
});

const target = {
  messageIds: ["msg-1", "msg-2"],
  rows: [],
  conversations: 1,
  threadId: "thread-1",
  primary: { id: "msg-2" },
  anyStarred: false,
  anyUnread: false,
  labels: [],
  source: "list",
} as unknown as MailTarget;

function renderDialog(ui: ReactNode) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);
}

describe("ReplyLaterDialog", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  test("a typed time sets the instant the preview showed, for the conversation", async () => {
    api.deferThreads.mockResolvedValue(true);
    const onClose = vi.fn<() => void>();
    renderDialog(
      <ReplyLaterDialog target={target} subject="Launch plan" waiting={false} onClose={onClose} />,
    );

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "in 2d" } });
    await screen.findByText(/in 2 days/);
    fireEvent.click(screen.getByRole("button", { name: "Set" }));

    await waitFor(() => expect(api.deferThreads).toHaveBeenCalledTimes(1));
    const [threads, choice] = api.deferThreads.mock.calls[0] ?? [];
    expect(threads).toEqual(["thread-1"]);
    expect(new Date(choice?.at ?? "").getTime() - Date.now()).toBeGreaterThan(47 * 3_600_000);
    expect(onClose).toHaveBeenCalled();
    expect(api.setReplyLater).not.toHaveBeenCalled();
  });
});
