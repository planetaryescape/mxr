import { describe, expect, test } from "vitest";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

import {
  EXPECTED_IPC_PROTOCOL_VERSION,
  evaluateProtocolCompatibility,
} from "./protocolCompatibility";
import { evaluateProtocolCompatibility as evaluateProtocolCompatibilityV4 } from "./__fixtures__/protocolCompatibility-v4";

describe("protocol compatibility", () => {
  test("accepts the expected bridge protocol", () => {
    expect(
      evaluateProtocolCompatibility({ protocol_version: EXPECTED_IPC_PROTOCOL_VERSION }),
    ).toBeUndefined();
  });

  test.each([4, 5, 6])("requires an update from the protocol %i bridge", (protocolVersion) => {
    expect(evaluateProtocolCompatibility({ protocol_version: protocolVersion })).toMatchObject({
      actualProtocol: protocolVersion,
      requiredProtocol: EXPECTED_IPC_PROTOCOL_VERSION,
      detail: "mxr Web needs a compatible bridge before it can safely use daemon APIs.",
    });
  });

  test("reports missing bridge protocols", () => {
    expect(evaluateProtocolCompatibility({})).toMatchObject({ actualProtocol: null });
  });

  test("an old web client rejects the new daemon through the health gate", () => {
    expect(evaluateProtocolCompatibilityV4({ protocol_version: 7 })).toMatchObject({
      actualProtocol: 7,
      requiredProtocol: 4,
    });
  });

  test("matches the Rust IPC protocol constant", () => {
    const testDir = dirname(fileURLToPath(import.meta.url));
    const protocolLib = resolve(testDir, "../../../..", "crates/protocol/src/lib.rs");
    const source = readFileSync(protocolLib, "utf8");
    const match = source.match(/pub const IPC_PROTOCOL_VERSION: u32 = (\d+);/);

    expect(match?.[1]).toBe(String(EXPECTED_IPC_PROTOCOL_VERSION));
  });
});
