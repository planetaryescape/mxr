import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DaemonUnavailableError } from "@/api/client";
import { draftFromSession } from "./composeDraft";

import type { ComposeSessionResponse } from "../api";

const api = vi.hoisted(() => ({
  refreshComposeSession: vi.fn<(draftPath: string) => Promise<ComposeSessionResponse>>(),
  restoreComposeSession: vi.fn<(draftId: string) => Promise<ComposeSessionResponse>>(),
  startComposeSession: vi.fn<() => Promise<ComposeSessionResponse>>(),
}));
vi.mock("../api", () => api);

const { loadInitialComposeSession, rememberActiveDraft } = await import("./activeDrafts");
const { requestCoordinator } = await import("@/lib/requestCoordinator");
const { composeQueueKey } = await import("./composeDraft");

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
  test.each(["recovery", "canonical"] as const)(
    "a %s outage retains the remembered recovery until reconnect",
    async (destination) => {
      const intent = {
        key: `compose:draft:outage-${destination}`,
        title: "Existing",
        kind: "new" as const,
        draftId: "existing",
      };
      const response: ComposeSessionResponse = {
        session: {
          draftId: "existing",
          draftPath: destination === "recovery" ? "/tmp/private-recovery" : "/tmp/canonical",
          recoverySourcePath: destination === "recovery" ? "/tmp/original" : undefined,
          revision: 3,
          accountId: "acct-1",
          bodyMarkdown: "Latest preserved text",
          rawContent: "",
          previewHtml: "",
          issues: [],
          frontmatter: {
            from: "me@example.com",
            to: "recipient@example.com",
            subject: "Preserved",
            attach: ["/tmp/private-recovery.attachments/report.txt"],
          },
        },
      };
      rememberActiveDraft(intent.key, {
        ...draftFromSession(response.session),
        preservedRecoveryPath: "/tmp/earlier-recovery",
      });
      const remembered = window.localStorage.getItem("mxr.compose.activeDrafts");
      const outage = new DaemonUnavailableError({ cause: new Error("connection lost") });
      const loader =
        destination === "recovery" ? api.refreshComposeSession : api.restoreComposeSession;
      loader.mockRejectedValueOnce(outage).mockResolvedValueOnce(response);

      await expect(loadInitialComposeSession(intent)).rejects.toBe(outage);
      expect(loader).toHaveBeenCalledTimes(1);
      expect(api.startComposeSession).not.toHaveBeenCalled();
      if (destination === "recovery") expect(api.restoreComposeSession).not.toHaveBeenCalled();
      else expect(api.refreshComposeSession).not.toHaveBeenCalled();
      expect(window.localStorage.getItem("mxr.compose.activeDrafts")).toBe(remembered);

      const loaded = await loadInitialComposeSession(intent);
      expect(loader).toHaveBeenLastCalledWith(
        destination === "recovery" ? "/tmp/private-recovery" : "existing",
      );
      expect(loaded.session).toEqual(response.session);
      expect(loaded.preservedRecoveryPath).toBe("/tmp/earlier-recovery");
      expect(window.localStorage.getItem("mxr.compose.activeDrafts")).toBe(remembered);
    },
  );

  test("a missing stored-draft recovery file falls back to the same canonical draft", async () => {
    const intent = {
      key: "compose:draft:existing",
      title: "Existing",
      kind: "new" as const,
      draftId: "existing",
    };
    rememberActiveDraft(intent.key, {
      draftPath: "/tmp/missing-recovery",
      recoverySourcePath: "/tmp/original",
      accountId: "acct-1",
    } as Parameters<typeof rememberActiveDraft>[1]);
    api.refreshComposeSession.mockRejectedValueOnce(new Error("file missing"));
    api.restoreComposeSession.mockResolvedValueOnce({
      session: {
        draftPath: "/tmp/canonical",
        draftId: "existing",
        bodyMarkdown: "canonical",
      } as ComposeSessionResponse["session"],
    });
    const loaded = await loadInitialComposeSession(intent);
    expect(api.restoreComposeSession).toHaveBeenCalledWith("existing");
    expect(api.startComposeSession).not.toHaveBeenCalled();
    expect(loaded.session.bodyMarkdown).toBe("canonical");
  });

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

  test("reopening waits for the closed composer's save, so A to B to A shows the latest text", async () => {
    const intent = {
      key: "compose:reply:a",
      title: "Reply",
      kind: "reply" as const,
      messageId: "a",
    };
    rememberActiveDraft(intent.key, {
      draftPath: "/tmp/a.md",
      accountId: "acct-1",
    } as Parameters<typeof rememberActiveDraft>[1]);
    // What the file holds: the older text until the save lands.
    let onDisk = "older reply";
    api.refreshComposeSession.mockImplementation(async () => ({
      session: {
        draftPath: "/tmp/a.md",
        bodyMarkdown: onDisk,
      } as ComposeSessionResponse["session"],
    }));
    let finishSave!: () => void;
    // The composer for A closed (focus moved to B) with its save in flight.
    const saving = requestCoordinator.queueComposeLatest(
      composeQueueKey("/tmp/a.md"),
      () =>
        new Promise<void>((resolve) => {
          finishSave = () => {
            onDisk = "newer reply";
            resolve();
          };
        }),
    );

    // Back to A before the save is done.
    const reopening = loadInitialComposeSession(intent);
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(api.refreshComposeSession).not.toHaveBeenCalled();

    finishSave();
    await saving;
    const loaded = await reopening;
    expect(loaded.session.bodyMarkdown).toBe("newer reply");
  });
});
