import { describe, expect, it } from "vitest";

import { snoozedEmptyBody } from "./SnoozedRoute";

describe("Snoozed's empty state", () => {
  it("names Not now's key, b, not the retired Z", () => {
    expect(snoozedEmptyBody()).toBe("Press b on a conversation to send it away until later.");
  });
});
