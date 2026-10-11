import { describe, expect, it } from "vitest";

import { BridgeRequestError, DaemonUnavailableError, UnauthorizedError } from "@/api/client";
import { describeRequestError } from "./RequestError";

describe("describeRequestError", () => {
  it("gives a restart action for an unavailable daemon and keeps its detail", () => {
    const result = describeRequestError(new DaemonUnavailableError());
    expect(result.summary).toContain("mxr restart");
    expect(result.technical).toContain("Couldn't reach");
  });

  it("maps authorization and service failures without losing technical detail", () => {
    expect(describeRequestError(new UnauthorizedError()).summary).toContain("Bridge token");
    const service = describeRequestError(new BridgeRequestError(503, "upstream timed out"));
    expect(service.summary).toContain("service error");
    expect(service.technical).toBe("upstream timed out");
  });

  it("does not trust malformed error values as display copy", () => {
    expect(describeRequestError({ message: "unexpected object" })).toEqual({
      summary: "mxr could not complete this request.",
      technical: "No additional detail was provided.",
    });
  });
});
