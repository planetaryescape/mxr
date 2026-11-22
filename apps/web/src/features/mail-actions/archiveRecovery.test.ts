import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  queryArchive,
  retainedArchives,
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
});
