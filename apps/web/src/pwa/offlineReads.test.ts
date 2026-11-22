import { describe, expect, test } from "vitest";

import { mayCacheMail } from "./offlineReads";

describe("offline read admission", () => {
  test("cannot cache authentication failures, redirects, or an HTML login page", () => {
    for (const status of [200, 302, 401, 403, 500]) {
      expect(mayCacheMail(new Response("login", { status }))).toBe(false);
    }
    expect(mayCacheMail(Response.json({ thread: {} }))).toBe(true);
    expect(mayCacheMail(Response.json({ error: "expired" }, { status: 401 }))).toBe(false);
  });
});
