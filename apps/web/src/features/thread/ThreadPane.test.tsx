/* @vitest-environment jsdom */

import { QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, test, vi } from "vitest";

import { ThreadPane } from "./ThreadPane";
import { BridgeRequestError } from "@/api/client";
import { createQueryClient } from "@/lib/queryClient";
import type { ThreadResponse } from "@/features/mailbox/types";
import type { ThreadModes } from "@/features/modes/membership";

const mailbox = vi.hoisted(() => ({
  fetchThread: vi.fn<(id: string) => Promise<ThreadResponse>>(),
}));
const membership = vi.hoisted(() => ({
  fetch: vi.fn<() => Promise<{ threads: ThreadModes[] }>>(),
}));
const context = vi.hoisted(() => ({ fetch: vi.fn<() => Promise<null>>() }));

vi.mock("./ThreadReader", () => ({ ThreadReader: () => <article>Ready to read</article> }));
vi.mock("@/api/client", async (original) => ({
  ...(await original<typeof import("@/api/client")>()),
  apiFetch: membership.fetch,
}));

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  Link: ({ children }: { children: ReactNode }) => <a href="/m/inbox">{children}</a>,
}));

vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  fetchThread: mailbox.fetchThread,
}));

vi.mock("./context/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./context/api")>()),
  threadContextQuery: (threadId: string) => ({
    queryKey: ["thread", threadId, "context"],
    queryFn: context.fetch,
    retry: false,
  }),
}));

vi.mock("@/features/llm/useLlmStatus", () => ({
  llmPolicyKey: () => "off",
  useLlmStatus: () => ({ enabled: false, data: undefined }),
}));

describe("ThreadPane", () => {
  test("the messages wait for their mode line, so its late arrival cannot move them", async () => {
    const thread: ThreadResponse = {
      thread: {
        id: "t1",
        account_id: "a1",
        latest_date: "2026-10-20T10:00:00Z",
        message_count: 0,
        participants: [],
        snippet: "",
        subject: "Test",
        unread_count: 0,
      },
      messages: [],
      bodies: [],
    };
    mailbox.fetchThread.mockResolvedValue(thread);
    context.fetch.mockResolvedValue(null);
    let release: ((value: { threads: ThreadModes[] }) => void) | undefined;
    membership.fetch.mockReturnValue(
      new Promise((resolve) => {
        release = resolve;
      }),
    );
    render(
      <QueryClientProvider client={createQueryClient()}>
        <ThreadPane threadId="t1" />
      </QueryClientProvider>,
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100));
    });
    expect(screen.queryByText("Ready to read")).toBeNull();
    await act(async () => {
      release?.({ threads: [] });
    });
    expect(await screen.findByText("Ready to read")).toBeTruthy();
  });

  test("a missing conversation shows its error at once, with no retries", async () => {
    mailbox.fetchThread.mockClear();
    context.fetch.mockReturnValue(new Promise(() => {}));
    mailbox.fetchThread.mockRejectedValue(new BridgeRequestError(404, "thread not found"));
    render(
      <QueryClientProvider client={createQueryClient()}>
        <ThreadPane threadId="missing-thread" />
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Couldn't find this conversation")).toBeTruthy();
    // A lookup that can't succeed offers the way out, never a retry.
    expect(screen.getByRole("link", { name: "Go to Inbox" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(mailbox.fetchThread).toHaveBeenCalledTimes(1);
  });
});
