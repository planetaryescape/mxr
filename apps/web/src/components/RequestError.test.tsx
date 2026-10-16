import { describe, expect, it } from "vitest";

import { BridgeRequestError, DaemonUnavailableError, UnauthorizedError } from "@/api/client";
import { explainError } from "./RequestError";

describe("explainError", () => {
  it("says the daemon isn't running, offers the start command and keeps the raw error", () => {
    const result = explainError(new DaemonUnavailableError());
    expect(result.heading).toBe("mxr isn't running");
    expect(result.fix).toBe("start");
    expect(result.technical).toContain("Couldn't reach");
  });

  it("sends a rejected token to settings and a protocol mismatch to an update", () => {
    expect(explainError(new UnauthorizedError()).fix).toBe("token");
    const mismatch = explainError(
      new BridgeRequestError(426, "Update or restart mxr", "protocol_mismatch"),
    );
    expect(mismatch.heading).toBe("mxr needs updating");
    expect(mismatch.fix).toBe("update");
  });

  it("offers the way out, never a retry, when a lookup can't succeed", () => {
    for (const error of [
      new BridgeRequestError(400, "invalid thread id: not-a-real-thread-id", "bad_request"),
      new BridgeRequestError(404, "thread not found", "not_found"),
    ]) {
      const result = explainError(error, "this conversation");
      expect(result.heading).toBe("Couldn't find this conversation");
      expect(result.fix).toBe("inbox");
      expect(result.technical).toBe(error.message);
    }
  });

  it("retries a timeout and a service error, and refuses a permission failure", () => {
    expect(explainError(new BridgeRequestError(504, "slow", "timeout")).fix).toBe("retry");
    const service = explainError(new BridgeRequestError(500, "boom", "internal"), "Messages");
    expect(service).toMatchObject({
      heading: "Couldn't load Messages",
      fix: "retry",
      generic: true,
    });
    const denied = explainError(new BridgeRequestError(403, "no", "policy"));
    expect(denied.fix).toBe("none");
    expect(denied.generic).toBe(false);
  });

  it("does not trust malformed error values as display copy", () => {
    expect(explainError({ message: "unexpected object" })).toMatchObject({
      heading: "Something went wrong",
      technical: "No additional detail was provided.",
      fix: "retry",
    });
  });
});
