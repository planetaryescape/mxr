import { describe, expect, test, vi } from "vitest";

const fetchMock = vi.hoisted(() =>
  vi.fn<(path: string, opts?: unknown) => Promise<unknown>>(async () => ({})),
);
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

import { draftAssistThread } from "./api";

describe("draftAssistThread", () => {
  test("posts to the unified compose endpoint with the instruction", async () => {
    fetchMock.mockClear();
    await draftAssistThread({ threadId: "t1", instruction: "reply yes" });
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/drafts/compose", {
      method: "POST",
      body: { thread_id: "t1", instruction: "reply yes" },
    });
  });

  test("includes register and length only when overridden", async () => {
    fetchMock.mockClear();
    await draftAssistThread({
      threadId: "t1",
      instruction: "reply",
      register: "formal",
      lengthHint: "short",
    });
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/drafts/compose", {
      method: "POST",
      body: { thread_id: "t1", instruction: "reply", register: "formal", length_hint: "short" },
    });
  });
});
