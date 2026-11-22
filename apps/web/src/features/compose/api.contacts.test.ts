/* @vitest-environment jsdom */

import { beforeEach, describe, expect, test, vi } from "vitest";

import { apiFetch } from "@/api/client";
import { fetchContactsAutocomplete } from "./api";

vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: vi.fn<typeof apiFetch>(),
}));

const apiFetchMock = vi.mocked(apiFetch);

describe("fetchContactsAutocomplete", () => {
  beforeEach(() => {
    apiFetchMock.mockReset();
  });

  test("drops rows without an address so the dropdown never receives them", async () => {
    apiFetchMock.mockResolvedValue({
      contacts: [{ name: "No Address" }, { email: "nora@foundry.example", name: null }],
    });

    await expect(fetchContactsAutocomplete("nor")).resolves.toEqual([
      { email: "nora@foundry.example", name: null },
    ]);
  });
});
