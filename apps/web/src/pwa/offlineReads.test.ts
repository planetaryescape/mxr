import { describe, expect, test } from "vitest";

import { isOfflineRead, isUnavailableResponse, mayCacheMail } from "./offlineReads";

const origin = "https://mxr.example.test";

describe("offline read admission", () => {
  test("uses saved mail for a disconnected tailnet, but respects mailbox authority", () => {
    expect(
      isUnavailableResponse(
        new Response(null, {
          status: 403,
          headers: { "x-mxr-tailnet-required": "1" },
        }),
      ),
    ).toBe(true);
    for (const status of [200, 401, 403]) {
      expect(isUnavailableResponse(new Response(null, { status }))).toBe(false);
    }
    for (const status of [502, 503, 504]) {
      expect(isUnavailableResponse(new Response(null, { status }))).toBe(true);
    }
  });
  test.each([
    "/api/v1/client/shell",
    "/api/v1/mail/mailbox?lens_kind=inbox",
    "/api/v1/mail/threads/123",
    "/api/v1/mail/threads/123/context",
    "/api/v1/mail/people",
  ])("can retain a displayed mail read: %s", (path) => {
    expect(isOfflineRead(new URL(path, origin), "GET", origin)).toBe(true);
  });

  test.each([
    "/api/v1/auth/local-token",
    "/api/v1/remote/browser-session",
    "/api/v1/platform/accounts/config",
    "/api/v1/mail/threads/123/summarize",
    "/api/v1/mail/mutations/archive",
    "/api/v1/mail/drafts",
    "/api/v1/health",
  ])("keeps authority and actions out of the cache: %s", (path) => {
    expect(isOfflineRead(new URL(path, origin), "GET", origin)).toBe(false);
  });

  test("does not retain writes or reads from a different mail host", () => {
    const path = "/api/v1/mail/threads/123";
    expect(isOfflineRead(new URL(path, origin), "POST", origin)).toBe(false);
    expect(isOfflineRead(new URL(path, "https://other.example.test"), "GET", origin)).toBe(false);
  });

  test("cannot cache authentication failures, redirects, or an HTML login page", () => {
    for (const status of [200, 302, 401, 403, 500]) {
      expect(mayCacheMail(new Response("login", { status }))).toBe(false);
    }
    expect(mayCacheMail(Response.json({ thread: {} }))).toBe(true);
    expect(mayCacheMail(Response.json({ error: "expired" }, { status: 401 }))).toBe(false);
  });
});
