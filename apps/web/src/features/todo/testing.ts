/* Test rows shaped like the daemon's, for To do's unit tests. */

import type { Todo, TodoRunway } from "./api";

export function todoFixture(overrides: Partial<Todo> = {}): Todo {
  return {
    id: "todo_bill",
    account_id: "acct",
    kind: "bill",
    verb: "pay",
    title: "Pay council tax",
    counterparty: "Camden Council",
    amount: { minor: 14_200, currency: "GBP", display: "£142.00" },
    due_at: "2026-10-09T22:59:59Z",
    act_by_at: "2026-10-07T22:59:59Z",
    surface_at: "2026-10-04T08:00:00Z",
    state: "open",
    origin: "schema",
    why: 'Here because: "payment due 9 October" (schema.org).',
    when_label: "act by Wed 7 Oct · due Fri 9 Oct",
    overdue: false,
    runway: 0.6,
    action: {
      kind: "open_email",
      label: "Open email to pay",
      message_id: "msg_bill",
      url: "https://www.camden.gov.uk/pay-council-tax",
      domain: "camden.gov.uk",
      trusted: false,
    },
    fields: [
      {
        field: "amount",
        source: "schema",
        checked: true,
        evidence: "142.00",
        source_label: "schema.org markup in the email",
      },
      {
        field: "due_at",
        source: "rule",
        checked: false,
        evidence: "09/10/2026",
        source_label: "a pattern in the email",
      },
      {
        field: "act_by_at",
        source: "table",
        checked: true,
        evidence: "the due date",
        source_label: "the lead-time table",
      },
    ],
    user_touched: false,
    thread_id: "thread_bill",
    source_message_id: "msg_bill",
    created_at: "2026-10-02T09:00:00Z",
    updated_at: "2026-10-02T09:00:00Z",
    ...overrides,
  };
}

export function runwayFixture(overrides: Partial<TodoRunway> = {}): TodoRunway {
  return {
    generated_at: "2026-10-05T09:00:00Z",
    header: "Things email asked you to do, ordered by when to act.",
    headline: "1 thing needs you. Pay council tax, act by Wed.",
    now: [todoFixture()],
    coming_up: [],
    later: [],
    whenever: [],
    done_this_week: [],
    catchup_count: 0,
    expired_since_last_looked: 0,
    first_run: { complete: true, scanned: 10 },
    ...overrides,
  };
}
