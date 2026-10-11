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

  test("rejects an older daemon after the category protocol change", () => {
    expect(
      evaluateProtocolCompatibility({ protocol_version: EXPECTED_IPC_PROTOCOL_VERSION - 1 }),
    ).toMatchObject({
      actualProtocol: EXPECTED_IPC_PROTOCOL_VERSION - 1,
      requiredProtocol: EXPECTED_IPC_PROTOCOL_VERSION,
    });
  });

  test("requires exact compatibility in both client and daemon upgrades", () => {
    // Old web clients expect 4 and reject the new daemon's 5 health response;
    // current clients likewise reject an old daemon's 4 response above.
    expect(evaluateProtocolCompatibility({ protocol_version: 4 })).toMatchObject({
      actualProtocol: 4,
      requiredProtocol: 5,
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
