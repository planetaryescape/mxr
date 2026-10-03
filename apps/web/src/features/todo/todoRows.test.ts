import { describe, expect, test } from "vitest";

import { runwayFixture, todoFixture } from "./testing";
import {
  linkLine,
  openCount,
  orderedFields,
  primaryAction,
  provenanceSummary,
  rowParty,
  runwayFill,
  runwayItems,
} from "./todoRows";

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
    // Even a row an older daemon marked trusted only opens the email.
    const trusted = todoFixture({
      action: { label: "Pay on camden.gov.uk", url: "https://x.example", trusted: true },
    });
    expect(primaryAction(trusted)).toMatchObject({ kind: "email", messageId: "msg_bill" });
  });

  test("the domain is said before Enter", () => {
    expect(linkLine(todoFixture())).toBe("The link in the email goes to camden.gov.uk.");
    expect(linkLine(todoFixture({ action: null }))).toBeNull();
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

  test("past the deadline there is no bar, and the fill stays between 0 and 1", () => {
    expect(runwayFill(todoFixture({ runway: 0.6 }))).toBe(0.6);
    expect(runwayFill(todoFixture({ runway: 1.4 }))).toBe(1);
    expect(runwayFill(todoFixture({ overdue: true, when_label: "was due Fri 2 Oct" }))).toBeNull();
    expect(runwayFill(todoFixture({ runway: null }))).toBeNull();
  });

  test("the provenance chip names the main source and counts fields to check", () => {
    expect(provenanceSummary(todoFixture().fields)).toEqual({ source: "schema.org", unchecked: 1 });
    expect(orderedFields(todoFixture().fields).map((field) => field.field)).toEqual([
      "amount",
      "due_at",
      "act_by_at",
    ]);
  });
});

describe("the runway", () => {
  const coming = todoFixture({ id: "todo_renew", title: "Renew car insurance" });
  const whenever = todoFixture({ id: "todo_form", title: "Send the signed form", due_at: null });
  const done = todoFixture({ id: "todo_done", state: "done" });
  const runway = runwayFixture({
    coming_up: [{ week_start: "2026-10-12", label: "wk of 12 Oct", todos: [coming] }],
    whenever: [whenever],
    done_this_week: [done],
  });

  test("rows run Now, Coming up by week, then the folded bands once opened", () => {
    const folded = runwayItems(runway, { whenever: false, done: false });
    expect(folded.map((item) => [item.band, item.todo.id, item.group])).toEqual([
      ["now", "todo_bill", undefined],
      ["coming", "todo_renew", "wk of 12 Oct"],
    ]);
    const open = runwayItems(runway, { whenever: true, done: true });
    expect(open.map((item) => item.band)).toEqual(["now", "coming", "whenever", "done"]);
  });

  test("hidden rows leave at once, and done rows don't count as open", () => {
    const items = runwayItems(runway, { whenever: true, done: false }, new Set(["todo_bill"]));
    expect(items.map((item) => item.todo.id)).toEqual(["todo_renew", "todo_form"]);
    expect(openCount(runway)).toBe(3);
  });
});
