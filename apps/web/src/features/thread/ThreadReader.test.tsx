/* @vitest-environment jsdom */

import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ThreadResponse } from "@/features/mailbox/types";
import { createQueryClient } from "@/lib/queryClient";
import { useMailboxPane } from "@/state/mailboxPaneStore";

import { ThreadReader } from "./ThreadReader";

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  Link: ({ children }: { children: ReactNode }) => <a href="/m/inbox">{children}</a>,
  useNavigate: () => vi.fn<() => void>(),
  useRouterState: ({ select }: { select: (state: unknown) => unknown }) =>
    select({ location: { pathname: "/m/inbox/thread-1", search: {} } }),
}));
vi.mock("@/features/llm/useLlmStatus", () => ({
  llmPolicyKey: () => "off",
  useLlmStatus: () => ({ enabled: false, data: undefined }),
}));
const mailbox = vi.hoisted(() => ({
  fetchThread: vi.fn<(id: string, page?: unknown) => Promise<ThreadResponse>>(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  fetchThread: mailbox.fetchThread,
}));

const thread: ThreadResponse = {
  thread: {
    account_id: "acct",
    id: "thread-1",
    latest_date: "2026-10-06T16:00:00Z",
    message_count: 0,
    participants: [],
    snippet: "",
    subject: "Contract renewal",
    unread_count: 0,
  },
  messages: [],
  bodies: [],
};

afterEach(() => {
  act(() => useMailboxPane.getState().setActivePane("mailbox"));
});

const client = createQueryClient();

function Harness({ open }: { open: boolean }) {
  return (
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <ThreadReader data={thread} />
      </TooltipProvider>
      {/* Opened by a shortcut: no trigger to hand focus back to. */}
      <Dialog open={open}>
        <DialogContent aria-describedby={undefined}>
          <DialogTitle>Keys</DialogTitle>
        </DialogContent>
      </Dialog>
    </QueryClientProvider>
  );
}

function row(id: string): ThreadResponse["messages"][number] {
  return {
    id,
    kind: "message",
    account_id: "acct",
    thread_id: "thread-1",
    provider_id: id,
    sender: "Sam <sam@example.com>",
    subject: "Contract renewal",
    snippet: id,
    date: "2026-10-06T16:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
  };
}

describe("ThreadReader", () => {
  test("prepends an earlier page without moving the reader or losing a message", async () => {
    const page: ThreadResponse = {
      thread: { ...thread.thread, message_ids: ["m0", "m1", "m2"] },
      messages: [row("m1"), row("m2")],
      bodies: [],
      total_messages: 3,
      has_earlier: true,
      earlier_before: "m1",
    };
    Element.prototype.scrollIntoView = vi.fn<() => void>();
    let resolveEarlier: ((value: ThreadResponse) => void) | undefined;
    mailbox.fetchThread.mockReturnValue(
      new Promise((resolve) => {
        resolveEarlier = resolve;
      }),
    );
    render(
      <QueryClientProvider client={createQueryClient()}>
        <TooltipProvider>
          <ThreadReader data={page} />
        </TooltipProvider>
      </QueryClientProvider>,
    );

    const scroll = screen.getByTestId("thread-scroll");
    let height = 1000;
    Object.defineProperty(scroll, "scrollHeight", {
      configurable: true,
      get: () => height,
    });
    scroll.scrollTop = 100;

    fireEvent.click(screen.getByTestId("thread-load-earlier"));
    expect(mailbox.fetchThread).toHaveBeenCalledWith("thread-1", {
      before: "m1",
      limit: 50,
    });

    // The prepended page adds 300px above the reader.
    height = 1300;
    await act(async () => {
      resolveEarlier?.({
        thread: page.thread,
        messages: [row("m0")],
        bodies: [],
        total_messages: 3,
        has_earlier: false,
      });
    });

    await waitFor(() =>
      expect(screen.getAllByTestId("thread-message").map((node) => node.dataset.messageId)).toEqual([
        "m0",
        "m1",
        "m2",
      ]),
    );
    await waitFor(() => expect(scroll.scrollTop).toBe(400));
    expect(screen.queryByTestId("thread-load-earlier")).toBeNull();
  });

  test("takes focus back when an overlay closes while it drives the keys", async () => {
    act(() => useMailboxPane.getState().setActivePane("reader"));
    const { rerender } = render(<Harness open />);
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));

    fireEvent.keyDown(dialog, { key: "Escape" });
    rerender(<Harness open={false} />);

    await waitFor(() =>
      expect(screen.getByRole("article", { name: "Conversation: Contract renewal" })).toHaveFocus(),
    );
  });
});
