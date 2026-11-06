import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  archiveMessagesRecoverably,
  needsRecovery,
  previewArchive,
  queryArchive,
  retainedArchives,
  retainArchive,
  submitArchive,
  type ArchiveOperation,
} from "./archiveRecovery";

function operation(): ArchiveOperation {
  return {
    identity: {
      operation_id: crypto.randomUUID(),
      caller_id: crypto.randomUUID(),
      workspace: { workspace_id: crypto.randomUUID(), recovery_generation: crypto.randomUUID() },
    },
    review_id: crypto.randomUUID(),
    mutation_id: crypto.randomUUID(),
    accepted: false,
    running: false,
    undo_available: false,
    started_at: 1,
    items: [
      {
        message_id: crypto.randomUUID(),
        account_id: crypto.randomUUID(),
        provider_id: "fake-1",
        was_inbox: true,
        prior_flags_bits: 0,
        prior_label_provider_ids: ["INBOX"],
        state: "pending",
        reason: null,
      },
    ],
  };
}
// Keep the storage contract stable on Node 26, which shadows jsdom storage.
beforeEach(() => {
  const items = new Map<string, string>();
  const storage: Storage = {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, String(value)),
  };
  vi.stubGlobal("localStorage", storage);
  localStorage.setItem("mxr.bridgeToken", "synthetic-test-token");
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
describe("Archive consuming client recovery", () => {
  test("a lost submit response retains the identity before dispatch and query uses it without submitting", async () => {
    const reviewed = operation();
    let submissions = 0;
    let queries = 0;
    const retainedAtDispatch: ArchiveOperation[] = [];
    const submittedBodies: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        if (String(input).endsWith("/submit")) {
          submissions += 1;
          retainedAtDispatch.push(...retainedArchives());
          submittedBodies.push(String(init?.body));
          throw new Error("reply lost after acceptance");
        }
        queries += 1;
        expect(String(input)).toContain("/query");
        expect(JSON.parse(String(init?.body)).identity).toEqual(reviewed.identity);
        return Response.json({
          kind: "ArchiveOperation",
          operation: {
            ...reviewed,
            accepted: true,
            items: reviewed.items.map((item) => ({ ...item, state: "unknown" })),
          },
        });
      }),
    );
    await expect(submitArchive(reviewed)).rejects.toThrow("Couldn't reach mxr's daemon");
    const retained = retainedArchives()[0];
    expect(retained).toEqual(reviewed);
    if (!retained) throw new Error("retained operation missing");
    const recovered = await queryArchive(retained);
    expect(recovered.items[0]?.state).toBe("unknown");
    expect(retainedAtDispatch).toEqual([reviewed]);
    expect(JSON.parse(submittedBodies[0] ?? "{}").identity).toEqual(reviewed.identity);
    expect(submissions).toBe(1);
    expect(queries).toBe(1);
    expect(retainedArchives()[0]?.identity).toEqual(reviewed.identity);
  });
  test("failed client persistence prevents submit and preserves an existing review", async () => {
    const reviewed = operation();
    const fetcher = vi.fn<() => Promise<Response>>();
    vi.stubGlobal("fetch", fetcher);
    vi.spyOn(localStorage, "setItem").mockImplementation(() => {
      throw new Error("storage unavailable");
    });
    await expect(submitArchive(reviewed)).rejects.toThrow("storage unavailable");
    expect(fetcher).not.toHaveBeenCalled();
  });
  test("retained reviews reject identity replacement and malformed journal data", () => {
    const reviewed = operation();
    retainArchive(reviewed);
    expect(() =>
      retainArchive({
        ...reviewed,
        identity: { ...reviewed.identity, caller_id: crypto.randomUUID() },
      }),
    ).toThrow("identity or review");
    expect(retainedArchives()[0]).toEqual(reviewed);
    const key = Array.from({ length: localStorage.length }, (_, index) =>
      localStorage.key(index),
    ).find((value) => value?.startsWith("mxr.archive.operation."));
    expect(key).toBeTruthy();
    if (!key) throw new Error("missing journal fixture");
    localStorage.setItem(key, JSON.stringify({ ...reviewed, items: [{ state: "invented" }] }));
    expect(() => retainedArchives()).toThrow("malformed");
  });
  test.each(["not_executed", "unknown"] as const)(
    "partial %s retains Undo and reports uncertainty truthfully",
    async (state) => {
      const reviewed = operation();
      const first = reviewed.items[0];
      if (!first) throw new Error("missing fixture item");
      reviewed.items.push({ ...first, message_id: crypto.randomUUID(), provider_id: "fake-2" });
      const outcome: ArchiveOperation = {
        ...reviewed,
        accepted: true,
        undo_available: true,
        items: [
          { ...first, state: "completed" },
          {
            ...first,
            message_id: reviewed.items[1]?.message_id ?? "missing",
            provider_id: "fake-2",
            state,
            reason:
              state === "unknown" ? "provider acceptance unproved" : "provider proved rejection",
          },
        ],
      };
      const confirm = vi.spyOn(window, "confirm");
      vi.stubGlobal(
        "fetch",
        vi.fn<typeof fetch>(async (input) => {
          const path = String(input);
          if (path.endsWith("/admin/status"))
            return Response.json({ workspace_owner_supported: true });
          if (path.endsWith("/workspace"))
            return Response.json({
              kind: "ArchiveWorkspace",
              workspace: reviewed.identity.workspace,
            });
          return Response.json({
            kind: "ArchiveOperation",
            operation: path.endsWith("/preview") ? reviewed : outcome,
          });
        }),
      );
      const response = await archiveMessagesRecoverably(
        reviewed.items.map((item) => item.message_id),
      );
      // Archive acts at once; the safety record is the retained journal, not a dialog.
      expect(confirm).not.toHaveBeenCalled();
      expect(response.ok).toBe(false);
      expect(response.result).toMatchObject({
        requested: 2,
        succeeded: 1,
        failed: 1,
        mutation_id: outcome.mutation_id,
      });
      expect(response.error).toContain(state);
      expect(response.error).toContain("Unknown effects may have applied");
      expect(retainedArchives()[0]?.identity).toEqual(reviewed.identity);
    },
  );
  test("an unsupported daemon response refuses preview before submission", async () => {
    const fetcher = vi.fn<() => Promise<Response>>(async () =>
      Response.json({ kind: "Error", error: "unsupported" }),
    );
    vi.stubGlobal("fetch", fetcher);
    await expect(previewArchive(["message"])).rejects.toThrow("does not support");
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(retainedArchives()).toEqual([]);
  });
});

describe("which retained Archives Archive recovery offers", () => {
  test("a finished Archive is not offered; an unaccepted, running or unconfirmed one is", () => {
    const withItems = (state: "completed" | "unknown"): ArchiveOperation => {
      const op = operation();
      op.accepted = true;
      for (const item of op.items) item.state = state;
      return op;
    };
    const finished = withItems("completed");
    expect(needsRecovery(finished)).toBe(false);

    expect(needsRecovery(operation())).toBe(true);
    expect(needsRecovery({ ...finished, running: true })).toBe(true);
    expect(needsRecovery(withItems("unknown"))).toBe(true);
  });
});
