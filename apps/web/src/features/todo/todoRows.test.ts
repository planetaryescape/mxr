import { describe, expect, test } from "vitest";

import { runwayFixture, todoFixture } from "./testing";
import {
  arrivedDay,
  linkLine,
  dueByToday,
  openCount,
  orderedFields,
  primaryAction,
  provenanceSummary,
  rowParty,
  rowSecondary,
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

  test("the provenance chip names the main source and counts fields to check", () => {
    expect(provenanceSummary(todoFixture().fields)).toEqual({ source: "schema.org", unchecked: 1 });
    expect(orderedFields(todoFixture().fields).map((field) => field.field)).toEqual([
      "amount",
      "due_at",
      "act_by_at",
    ]);
  });
});

describe("a To do row's words", () => {
  const now = new Date(2026, 9, 14, 15, 0); // Wed 14 Oct 2026, local

  test("says the day an email arrived, in the fewest words", () => {
    expect(arrivedDay(new Date(2026, 9, 14, 8), now)).toBe("today");
    expect(arrivedDay(new Date(2026, 9, 13, 23), now)).toBe("yesterday");
    expect(arrivedDay(new Date(2026, 9, 12, 9), now)).toBe(
      new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(new Date(2026, 9, 12)),
    );
    expect(arrivedDay(new Date(2026, 8, 1, 9), now)).toBe(
      new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" }).format(
        new Date(2026, 8, 1),
      ),
    );
    expect(arrivedDay(new Date(2025, 8, 1, 9), now)).toBe(
      new Intl.DateTimeFormat(undefined, {
        day: "numeric",
        month: "short",
        year: "numeric",
      }).format(new Date(2025, 8, 1)),
    );
  });

  test("the secondary line is who, then when the email arrived; a promise is from you", () => {
    const arrived = new Date(2026, 9, 12, 9).toISOString();
    expect(
      rowSecondary(todoFixture({ source_date: arrived, counterparty: "Frank Billing" }), now),
    ).toMatch(/^Frank Billing · arrived \w+/);
    expect(
      rowSecondary(
        todoFixture({ kind: "promise", counterparty: "Quinn Dev", source_date: arrived }),
        now,
      ),
    ).toBe("Quinn Dev · from you");
    expect(rowSecondary(todoFixture({ counterparty: null, source_date: null }), now)).toBeNull();
  });

  test("an email filed elsewhere says so without hiding when it arrived", () => {
    const arrived = new Date(2026, 9, 12, 9).toISOString();
    const filed = new Date(2026, 9, 13, 9).toISOString();
    expect(
      rowSecondary(
        todoFixture({
          source_date: arrived,
          source_filed_at: filed,
          counterparty: "Klarna",
        }),
        now,
      ),
    ).toBe(
      `Klarna · arrived ${arrivedDay(new Date(arrived), now)} · left the inbox ${arrivedDay(
        new Date(filed),
        now,
      )}`,
    );
  });

  test("an offer or a looks-done sign joins the arrival instead of taking its place", () => {
    const arrived = new Date(2026, 9, 12, 9).toISOString();
    const day = arrivedDay(new Date(arrived), now);
    const pending = todoFixture({
      source_date: arrived,
      resolution: {
        id: "r",
        message_id: "m",
        thread_id: "t",
        source_date: "2026-10-04T09:00:00Z",
        quote: "Paid.",
        quote_location: "body",
        source_method: "rule",
        action: "complete",
        proposed_due_at: null,
        proposed_due_words: null,
        status: "pending",
      },
    });
    expect(rowSecondary(pending, now)).toBe(
      `Camden Council · arrived ${day} · a later email may change this`,
    );
    expect(
      rowSecondary(
        todoFixture({
          source_date: arrived,
          looks_done: { reason: "you replied", message_id: "m" },
        }),
        now,
      ),
    ).toBe(`Camden Council · arrived ${day} · looks done: you replied`);
  });

  test("only a date due or to act by today or earlier takes the cue colour", () => {
    expect(dueByToday(todoFixture({ overdue: true }), now)).toBe(true);
    expect(
      dueByToday(
        todoFixture({
          overdue: false,
          due_at: new Date(2026, 9, 14, 20).toISOString(),
          act_by_at: null,
        }),
        now,
      ),
    ).toBe(true);
    expect(
      dueByToday(
        todoFixture({
          overdue: false,
          due_at: new Date(2026, 9, 16, 9).toISOString(),
          act_by_at: null,
        }),
        now,
      ),
    ).toBe(false);
    expect(dueByToday(todoFixture({ overdue: false, due_at: null, act_by_at: null }), now)).toBe(
      false,
    );
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
