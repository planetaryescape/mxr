import { describe, expect, test } from "vitest";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

import {
  EXPECTED_IPC_PROTOCOL_VERSION,
  evaluateProtocolCompatibility,
} from "./protocolCompatibility";

describe("protocol compatibility", () => {
  test.each([4, 11])("requires an update from the protocol %i bridge", (protocolVersion) => {
    expect(evaluateProtocolCompatibility({ protocol_version: protocolVersion })).toMatchObject({
      actualProtocol: protocolVersion,
      requiredProtocol: EXPECTED_IPC_PROTOCOL_VERSION,
      detail: "mxr Web needs a compatible bridge before it can safely use daemon APIs.",
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
