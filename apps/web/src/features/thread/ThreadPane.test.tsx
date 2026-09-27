/* @vitest-environment jsdom */

import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { ThreadPane } from "./ThreadPane";
import { BridgeRequestError } from "@/api/client";
import { createQueryClient } from "@/lib/queryClient";

const mailbox = vi.hoisted(() => ({ fetchThread: vi.fn<(id: string) => Promise<unknown>>() }));

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
    expect(await screen.findByText("Couldn't open this conversation")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
    expect(mailbox.fetchThread).toHaveBeenCalledTimes(1);
  });
});
