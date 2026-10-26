/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { OrphanedDrafts } from "./OrphanedDrafts";
import { fetchSendOutcome, readSendReferences, recoverSendOutcome } from "./sendOutcomes";

vi.mock("./api", () => ({
  fetchOrphanedDrafts: vi.fn<typeof import("./api").fetchOrphanedDrafts>().mockResolvedValue([]),
}));
vi.mock("./sendOutcomes", () => ({
  fetchSendOutcome: vi.fn<typeof import("./sendOutcomes").fetchSendOutcome>(),
  readSendReferences: vi.fn<typeof import("./sendOutcomes").readSendReferences>(),
  recoverSendOutcome: vi.fn<typeof import("./sendOutcomes").recoverSendOutcome>(),
}));
const reference = {
  operationId: "synthetic-draft:2",
  accountId: "synthetic-account",
  draftId: "synthetic-draft",
};
const unknown = {
  identity: { operation_id: reference.operationId, expected_revision: 2 },
  state: "unknown" as const,
  local_message_id: null,
};

beforeEach(() => {
  vi.mocked(readSendReferences).mockReturnValue([reference]);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <OrphanedDrafts />
    </QueryClientProvider>,
  );
}

test("waiting approval reports the retry deadline rather than rejection or uncertainty", async () => {
  vi.mocked(fetchSendOutcome).mockResolvedValue({
    outcome: {
      ...unknown,
      state: "waiting",
      recovery: { retry_at: "2026-10-26T12:15:00Z", attempts: 1, phase: "cooldown", reason: null },
    },
  });
  show();
  expect(
    await screen.findByText(
      /Waiting for the mail provider; retrying automatically at 2026-10-26T12:15:00Z/,
    ),
  ).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Resume original approved send" })).toBeNull();
  expect(recoverSendOutcome).not.toHaveBeenCalled();
});

test("legacy recovery previews before confirming the exact evidence fingerprint", async () => {
  vi.mocked(fetchSendOutcome).mockResolvedValue({ outcome: unknown });
  vi.mocked(recoverSendOutcome)
    .mockResolvedValueOnce({
      outcome: unknown,
      recoverable: true,
      reason: "local_gmail_gate_proves_non_submission",
      evidence_sha256: "synthetic-proof-hash",
    })
    .mockResolvedValueOnce({ outcome: { ...unknown, state: "waiting" } });
  show();
  fireEvent.click(await screen.findByRole("button", { name: "Preview recovery" }));
  await waitFor(() => expect(recoverSendOutcome).toHaveBeenCalledWith(reference, undefined));
  expect(await screen.findByText(/No mail sent/)).toBeTruthy();
  fireEvent.click(await screen.findByRole("button", { name: "Resume original approved send" }));
  await waitFor(() =>
    expect(recoverSendOutcome).toHaveBeenCalledWith(reference, "synthetic-proof-hash"),
  );
  expect(recoverSendOutcome).toHaveBeenCalledTimes(2);
});
