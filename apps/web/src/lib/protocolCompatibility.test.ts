import { describe, expect, test } from "vitest";

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
});
