/* @vitest-environment jsdom */

import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, test, vi } from "vitest";

import { ThreadPane } from "./ThreadPane";
import { BridgeRequestError } from "@/api/client";
import { createQueryClient } from "@/lib/queryClient";

const mailbox = vi.hoisted(() => ({ fetchThread: vi.fn<(id: string) => Promise<unknown>>() }));

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  Link: ({ children }: { children: ReactNode }) => <a href="/m/inbox">{children}</a>,
}));

vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  fetchThread: mailbox.fetchThread,
}));

// The context request never answers: the thread's own error must not wait on it.
vi.mock("./context/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./context/api")>()),
  threadContextQuery: (threadId: string) => ({
    queryKey: ["thread", threadId, "context"],
    queryFn: () => new Promise(() => {}),
    retry: false,
  }),
}));

vi.mock("@/features/llm/useLlmStatus", () => ({
  llmPolicyKey: () => "off",
  useLlmStatus: () => ({ enabled: false, data: undefined }),
}));

describe("ThreadPane", () => {
  test("a missing conversation shows its error at once, with no retries", async () => {
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
