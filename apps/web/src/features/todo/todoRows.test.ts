import { describe, expect, test } from "vitest";

import { todoFixture } from "./testing";
import { primaryAction, rowParty } from "./todoRows";

describe("a To do row", () => {
  test("a row about a link opens its email, never the link", () => {
    const action = primaryAction(todoFixture());
    expect(action).toEqual({
      kind: "email",
      label: "Open email to pay",
      url: "https://www.camden.gov.uk/pay-council-tax",
      domain: "camden.gov.uk",
      messageId: "msg_bill",
    });
    const trusted = todoFixture({
      action: { label: "Pay on camden.gov.uk", url: "https://x.example", trusted: true },
    });
    expect(primaryAction(trusted)).toMatchObject({ kind: "email", messageId: "msg_bill" });
  });

  test("a promise replies to the person, and a row with nothing to point at opens the email", () => {
    const promise = todoFixture({
      kind: "promise",
      action: null,
      counterparty: "Priya Shah",
      person_label: "you promised Priya Shah",
    });
    expect(primaryAction(promise)).toEqual({
      kind: "reply",
      label: "Reply to Priya",
      messageId: "msg_bill",
    });
    expect(rowParty(promise)).toBe("you promised Priya Shah");
    expect(primaryAction(todoFixture({ kind: "rsvp", action: null }))?.label).toBe(
      "Open the invite",
    );
    expect(primaryAction(todoFixture({ action: null, thread_id: null }))).toBeNull();
  });
});
