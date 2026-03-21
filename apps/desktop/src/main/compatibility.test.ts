import { describe, expect, it } from "vitest";
import {
  buildUpdateSteps,
  evaluateCompatibility,
  parseStatusOutput,
} from "./compatibility.js";

describe("compatibility", () => {
  it("parses protocol status output", () => {
    expect(
      parseStatusOutput('{"protocol_version":1,"daemon_version":"0.4.4"}'),
    ).toEqual({
      protocol_version: 1,
      daemon_version: "0.4.4",
      daemon_build_id: null,
    });
  });

  it("returns mismatch when protocols differ", () => {
    const mismatch = evaluateCompatibility({
      expectedProtocol: 2,
      actual: {
        protocol_version: 1,
        daemon_version: "0.4.2",
      },
      binaryPath: "/usr/local/bin/mxr",
      usingBundled: false,
    });

    expect(mismatch?.kind).toBe("mismatch");
    expect(mismatch?.requiredProtocol).toBe(2);
    expect(mismatch?.actualProtocol).toBe(1);
    expect(mismatch?.updateSteps).toEqual(buildUpdateSteps());
  });
});
