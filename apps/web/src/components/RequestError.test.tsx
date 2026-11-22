import { describe, expect, it } from "vitest";

import { BridgeRequestError, UnauthorizedError } from "@/api/client";
import { explainError } from "./RequestError";

describe("explainError", () => {
  it("sends a rejected token to settings and a protocol mismatch to an update", () => {
    expect(explainError(new UnauthorizedError()).fix).toBe("token");
    const mismatch = explainError(
      new BridgeRequestError(426, "Update or restart mxr", "protocol_mismatch"),
    );
    expect(mismatch.heading).toBe("mxr needs updating");
    expect(mismatch.fix).toBe("update");
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
});
