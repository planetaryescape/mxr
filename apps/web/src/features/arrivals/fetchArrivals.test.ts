import { beforeEach, expect, test, vi } from "vitest";

import { fetchArrivals } from "./api";

const fetchMock = vi.hoisted(() => vi.fn<(path: string) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

beforeEach(() => {
  fetchMock.mockReset();
  fetchMock.mockResolvedValue({ arrivals: {} });
});

test("an opening visit marks and names no window", async () => {
  await fetchArrivals(null, true, "2026-10-07T08:00:00Z");
  expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/arrivals?mark_seen=true");
});

test("a refetch while Now stays open names the window the visit opened with", async () => {
  await fetchArrivals(null, false, "2026-10-07T08:00:00Z");
  expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/arrivals?since=2026-10-07T08%3A00%3A00Z");
});

test("a peek that holds no window names none", async () => {
  await fetchArrivals("acct", false);
  expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/arrivals?account=acct");
});
