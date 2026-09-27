import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { ComposeSessionResponse } from "../api";

const api = vi.hoisted(() => ({
  refreshComposeSession: vi.fn<(draftPath: string) => Promise<ComposeSessionResponse>>(),
  restoreComposeSession: vi.fn<(draftId: string) => Promise<ComposeSessionResponse>>(),
  startComposeSession: vi.fn<() => Promise<ComposeSessionResponse>>(),
}));
vi.mock("../api", () => api);

const { loadInitialComposeSession, rememberActiveDraft } = await import("./activeDrafts");

// Node's own inert localStorage can shadow jsdom's; give the test a real one.
function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, String(value)),
  };
}

beforeEach(() => {
  vi.stubGlobal("localStorage", memoryStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("resuming a compose session", () => {
  test("keeps the account the draft was written from, which the file doesn't name", async () => {
    const intent = {
      key: "compose:reply:m1",
      title: "Reply",
      kind: "reply" as const,
      messageId: "m1",
    };
    rememberActiveDraft(intent.key, {
      draftPath: "/tmp/draft.md",
      accountId: "acct-1",
    } as Parameters<typeof rememberActiveDraft>[1]);
    api.refreshComposeSession.mockResolvedValue({
      session: {
        draftPath: "/tmp/draft.md",
        bodyMarkdown: "Kept",
      } as ComposeSessionResponse["session"],
    });

    const loaded = await loadInitialComposeSession(intent);

    expect(api.refreshComposeSession).toHaveBeenCalledWith("/tmp/draft.md");
    expect(loaded.session.accountId).toBe("acct-1");
    expect(loaded.session.bodyMarkdown).toBe("Kept");
  });
});
