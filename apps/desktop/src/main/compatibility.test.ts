import { describe, expect, it } from "vitest";
import {
  assertBridgeMailboxContract,
  buildUpdateSteps,
  evaluateCompatibility,
  parseStatusOutput,
} from "./compatibility.js";

describe("compatibility", () => {
  it("parses protocol status output", () => {
    expect(parseStatusOutput('{"protocol_version":1,"daemon_version":"0.4.4"}')).toEqual({
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

  it("accepts the current mailbox bridge payload", () => {
    expect(() =>
      assertBridgeMailboxContract({
        shell: {
          accountLabel: "Personal",
          syncLabel: "Synced",
          statusMessage: "Ready",
          commandHint: "Ctrl-p",
        },
        sidebar: { sections: [] },
        mailbox: { lensLabel: "Inbox", counts: { unread: 0, total: 0 }, groups: [] },
      }),
    ).not.toThrow();
  });

  it("rejects the legacy mailbox bridge payload", () => {
    expect(() =>
      assertBridgeMailboxContract({
        envelopes: [],
      }),
    ).toThrow(/legacy \/mailbox payload/);
  });

  it("returns null when protocols match", () => {
    const result = evaluateCompatibility({
      expectedProtocol: 1,
      actual: {
        protocol_version: 1,
        daemon_version: "0.4.4",
      },
      binaryPath: "/usr/local/bin/mxr",
      usingBundled: true,
    });

    expect(result).toBeNull();
  });

  it("returns mismatch when actual status is null", () => {
    const result = evaluateCompatibility({
      expectedProtocol: 1,
      actual: null,
      binaryPath: "/usr/local/bin/mxr",
      usingBundled: false,
    });

    expect(result?.kind).toBe("mismatch");
    expect(result?.actualProtocol).toBeNull();
    expect(result?.daemonVersion).toBeNull();
  });

  it("throws on invalid JSON in parseStatusOutput", () => {
    expect(() => parseStatusOutput("not json")).toThrow();
  });

  it("throws when protocol_version is missing from status output", () => {
    expect(() => parseStatusOutput('{"daemon_version":"0.4.4"}')).toThrow(
      /missing protocol_version/,
    );
  });

  it("defaults daemon_build_id to null when absent", () => {
    const result = parseStatusOutput('{"protocol_version":2,"daemon_version":"0.5.0"}');
    expect(result.daemon_build_id).toBeNull();
    expect(result.protocol_version).toBe(2);
    expect(result.daemon_version).toBe("0.5.0");
  });

  it("rejects mailbox payload missing sidebar.sections array", () => {
    expect(() =>
      assertBridgeMailboxContract({
        mailbox: { groups: [] },
        sidebar: {},
        shell: {},
      }),
    ).toThrow(/missing sidebar\.sections/);
  });
});
