import { expect, test } from "vitest";
import { BridgeRequestError, DaemonUnavailableError } from "@/api/client";
import { acceptedDraftRevision } from "./acceptedDraftRevision";

for (const evidence of [
  null,
  [],
  { draft_id: "draft", revision: "2" },
  { draft_id: 3, revision: 2 },
  { draft_id: "draft", revision: 0 },
  { draft_id: "draft", revision: 1.5 },
]) {
  test(`rejects malformed accepted evidence ${JSON.stringify(evidence)}`, () => {
    const error = new BridgeRequestError(500, "failed", "internal", { accepted_draft: evidence });
    expect(acceptedDraftRevision(error)).toBeUndefined();
  });
}

test("unwraps accepted connection-error evidence without changing the original error", () => {
  const original = new BridgeRequestError(502, "provider connection failed", "connect", {
    accepted_draft: { draft_id: "draft", revision: 2 },
    details: { current_revision: 8 },
  });
  const wrapped = new DaemonUnavailableError({ cause: original });
  expect(acceptedDraftRevision(wrapped)).toEqual({ draft_id: "draft", revision: 2 });
  expect(wrapped.cause).toBe(original);
  expect(original.status).toBe(502);
  expect(original.code).toBe("connect");
  expect(original.message).toBe("provider connection failed");
  expect(original.details.details).toEqual({ current_revision: 8 });
});

test("an unavailable transport provides no acceptance evidence", () => {
  expect(
    acceptedDraftRevision(new DaemonUnavailableError({ cause: new TypeError("network") })),
  ).toBeUndefined();
});
