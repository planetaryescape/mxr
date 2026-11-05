import { afterEach, describe, expect, test, vi } from "vitest";

import { fetchSearch } from "./api";

const api = vi.hoisted(() => ({
  fetch: vi.fn<(path: string, opts?: unknown) => Promise<unknown>>(),
}));

vi.mock("@/api/client", () => ({
  apiFetch: api.fetch,
}));

function requestedQuery(): URLSearchParams {
  const path = String(api.fetch.mock.calls.at(-1)?.[0]);
  return new URLSearchParams(path.split("?")[1] ?? "");
}

describe("fetchSearch query", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  test("omitted sort and limit are left off, so the daemon applies its [search] defaults", async () => {
    api.fetch.mockResolvedValue({});

    await fetchSearch({ q: "invoice" });

    const query = requestedQuery();
    expect(query.has("sort")).toBe(false);
    expect(query.has("limit")).toBe(false);
    expect(query.get("q")).toBe("invoice");
  });

  test("an explicit sort and limit are sent as chosen", async () => {
    api.fetch.mockResolvedValue({});

    await fetchSearch({ q: "invoice", sort: "oldest", limit: 100 });

    const query = requestedQuery();
    expect(query.get("sort")).toBe("oldest");
    expect(query.get("limit")).toBe("100");
  });

  test("best match goes to the bridge as relevant", async () => {
    api.fetch.mockResolvedValue({});

    await fetchSearch({ q: "invoice", sort: "relevance" });

    expect(requestedQuery().get("sort")).toBe("relevant");
  });
});
