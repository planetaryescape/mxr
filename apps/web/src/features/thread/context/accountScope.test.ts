import { beforeEach, describe, expect, test, vi } from "vitest";

import { getThreadBriefing, summarizeThread } from "@/features/mailbox/api";

import { fetchThreadContext, fetchThreadGist, fetchThreadGists } from "./api";

const apiFetch = vi.hoisted(() =>
  vi.fn<(path: string, opts?: { body?: Record<string, unknown> }) => Promise<unknown>>(),
);
vi.mock("@/api/client", () => ({ apiFetch }));

beforeEach(() => {
  apiFetch.mockReset();
  apiFetch.mockResolvedValue({
    context: {},
    gist: {},
    batch: {},
    briefing: {},
  });
});

const urls = () => apiFetch.mock.calls.map((call) => call[0]);

describe("thread requests for an id held by two accounts name the row's account", () => {
  test("context, gist, summary and briefing carry ?account_id=", async () => {
    await fetchThreadContext("shared", "B");
    await fetchThreadGist("shared", true, "B");
    await summarizeThread("shared", "B");
    await getThreadBriefing({
      threadId: "shared",
      accountId: "B",
      refresh: true,
    });
    expect(urls()).toEqual([
      "/api/v1/mail/threads/shared/context?account_id=B",
      "/api/v1/mail/threads/shared/context/gist?refresh=true&account_id=B",
      "/api/v1/mail/threads/shared/summarize?account_id=B",
      "/api/v1/mail/threads/shared/briefing?refresh=true&account_id=B",
    ]);
  });

  test("a row with no account keeps the bare request", async () => {
    await fetchThreadContext("old");
    await fetchThreadGist("old");
    await getThreadBriefing({ threadId: "old" });
    expect(urls()).toEqual([
      "/api/v1/mail/threads/old/context",
      "/api/v1/mail/threads/old/context/gist",
      "/api/v1/mail/threads/old/briefing",
    ]);
  });

  test("batch gists send one pair per account", async () => {
    await fetchThreadGists(
      [
        { thread_id: "shared", account_id: "A" },
        { thread_id: "shared", account_id: "B" },
      ],
      true,
    );
    expect(apiFetch.mock.calls[0]?.[1]?.body).toMatchObject({
      thread_ids: ["shared"],
      thread_accounts: [
        { thread_id: "shared", account_id: "A" },
        { thread_id: "shared", account_id: "B" },
      ],
    });
  });
});
