import { describe, expect, test } from "vitest";

import { claimAccountReauthRequest, requestAccountReauth } from "./reauthRequest";

describe("account reauth request handoff", () => {
  test("claims and clears a pending request by account id", () => {
    window.sessionStorage.clear();
    requestAccountReauth("account-1");

    expect(claimAccountReauthRequest({ account_id: "account-1", key: "personal" })).toBe(true);
    expect(claimAccountReauthRequest({ account_id: "account-1", key: "personal" })).toBe(false);
  });

  test("claims a pending request by account key", () => {
    window.sessionStorage.clear();
    requestAccountReauth("personal");

    expect(claimAccountReauthRequest({ account_id: "account-1", key: "personal" })).toBe(true);
  });
});
