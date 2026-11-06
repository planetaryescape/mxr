import { describe, expect, it } from "vitest";
import { llmPolicyKey, type LlmStatus } from "./useLlmStatus";

function status(overrides: Partial<LlmStatus> = {}): LlmStatus {
  return {
    enabled: true,
    provider: "openai_compatible",
    model: "fixture-model",
    configured_model: "fixture-model",
    base_url: null,
    api_key_env: null,
    api_key_present: true,
    context_window: 8192,
    supports_streaming: true,
    request_timeout_secs: 30,
    ...overrides,
  };
}

describe("model output cache policy", () => {
  it("invalidates cached output when a redacted host endpoint changes", () => {
    const first = status({ policy_key: "first-host-policy" });
    const changed = status({ policy_key: "changed-host-policy" });
    expect(llmPolicyKey(first)).not.toBe(llmPolicyKey(changed));
    expect(llmPolicyKey(first)).toBe(llmPolicyKey({ ...first }));
  });

  it("retains local endpoint and disabled-model invalidation", () => {
    expect(llmPolicyKey(status({ base_url: "https://first.invalid" }))).not.toBe(
      llmPolicyKey(status({ base_url: "https://changed.invalid" })),
    );
    expect(llmPolicyKey(status({ enabled: false }))).toBe("off");
  });
});
