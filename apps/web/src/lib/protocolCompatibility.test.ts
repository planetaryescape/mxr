import { describe, expect, test } from "vitest";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

import {
  EXPECTED_IPC_PROTOCOL_VERSION,
  evaluateProtocolCompatibility,
} from "./protocolCompatibility";

describe("protocol compatibility", () => {
  test("accepts the expected bridge protocol", () => {
    expect(
      evaluateProtocolCompatibility({ protocol_version: EXPECTED_IPC_PROTOCOL_VERSION }),
    ).toBeUndefined();
  });

  test("reports missing or mismatched bridge protocols", () => {
    expect(evaluateProtocolCompatibility({ protocol_version: 1 })).toMatchObject({
      actualProtocol: 1,
      requiredProtocol: EXPECTED_IPC_PROTOCOL_VERSION,
    });
    expect(evaluateProtocolCompatibility({})).toMatchObject({ actualProtocol: null });
  });

  test("matches the Rust IPC protocol constant", () => {
    const testDir = dirname(fileURLToPath(import.meta.url));
    const protocolLib = resolve(testDir, "../../../..", "crates/protocol/src/lib.rs");
    const source = readFileSync(protocolLib, "utf8");
    const match = source.match(/pub const IPC_PROTOCOL_VERSION: u32 = (\d+);/);

    expect(match?.[1]).toBe(String(EXPECTED_IPC_PROTOCOL_VERSION));
  });
});
