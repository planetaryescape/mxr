import { describe, expect, it } from "vitest";
import { redactSentryEvent } from "./telemetry-redaction.js";

describe("telemetry redaction", () => {
  it("removes high-risk event fields and redacts personal strings", () => {
    const event = redactSentryEvent({
      message: "Failed for alice@example.com at /Users/alice/project",
      request: { headers: { authorization: "Bearer secret" } },
      extra: { subject: "Launch" },
      breadcrumbs: [{ message: "Opened message" }],
      user: { email: "alice@example.com" },
      exception: {
        values: [
          {
            stacktrace: {
              frames: [{ filename: "/home/alice/mxr/src/main.ts" }],
            },
          },
        ],
      },
    });

    expect(event.request).toBeUndefined();
    expect(event.extra).toBeUndefined();
    expect(event.breadcrumbs).toBeUndefined();
    expect(event.user).toBeUndefined();
    expect(event.message).toBe("Failed for [email] at /Users/[user]/project");
    expect(event.exception.values[0].stacktrace.frames[0].filename).toBe(
      "/home/[user]/mxr/src/main.ts",
    );
  });
});
