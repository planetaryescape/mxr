import { describe, expect, test } from "vitest";

import { parseFrame } from "./ws";

describe("parseFrame", () => {
  test("a SyncError frame reaches the handlers as a sync failure, not a lost bridge", () => {
    const frame = parseFrame(
      JSON.stringify({
        event: "SyncError",
        account_id: "a-1",
        error: "Provider error: rate limited",
      }),
    );
    expect(frame).toEqual({
      kind: "event",
      event: {
        event: "SyncError",
        type: "SyncError",
        account_id: "a-1",
        error: "Provider error: rate limited",
      },
    });
  });

  test("an OperationFailed frame reaches the handlers too", () => {
    const frame = parseFrame(
      JSON.stringify({
        event: "OperationFailed",
        operation_id: "op-1",
        operation: "sync",
        account_id: "a-1",
        error: "connection refused",
        retryable: true,
      }),
    );
    expect(frame.kind).toBe("event");
    expect(frame.kind === "event" && frame.event.type).toBe("OperationFailed");
  });

  test("only a bare error frame means the bridge lost the daemon", () => {
    expect(parseFrame(JSON.stringify({ error: "daemon unreachable" }))).toEqual({
      kind: "bridge-error",
      error: "daemon unreachable",
    });
    expect(parseFrame("not json")).toEqual({ kind: "ignore" });
    expect(parseFrame(JSON.stringify({ hello: 1 }))).toEqual({ kind: "ignore" });
  });
});
