import { describe, expect, it } from "vitest";

import { fakeChoice } from "@/features/time/testing";

import { deferredMessage } from "./deskLater";

describe("deferredMessage", () => {
  const choice = fakeChoice(new Date(2026, 9, 6, 9, 0), {
    date_label: "Tuesday 6 October",
    time_label: "09:00",
  });

  it("names the exact time reply later comes back", () => {
    expect(deferredMessage(["reply_later"], choice)).toBe(
      "Reply later: back Tuesday 6 October, 09:00",
    );
  });

  it("says a wait comes back only if nobody replies", () => {
    expect(deferredMessage(["waiting"], choice)).toBe(
      "Back Tuesday 6 October, 09:00 if nobody replies",
    );
  });
});
