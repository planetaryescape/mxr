/* Records shaped like the daemon's, for Archive's unit tests. */

import type {
  RecordAnswer,
  RecordAnswerList,
  RecordData,
  RecordMonth,
  RecordSubscription,
  RecordSubscriptions,
} from "./api";

export function recordFixture(overrides: Partial<RecordData> = {}): RecordData {
  return {
    id: "rec_tap",
    account_id: "acct",
    kind: "booking",
    kind_label: "Booking",
    issuer: "TAP Air Portugal",
    title: "LHR -> LIS TP1357",
    reference: "K7QX2M",
    reference_label: "Booking ref",
    amount: { minor: 21_240, currency: "GBP", display: "£212.40" },
    date: "2025-06-02T12:00:00Z",
    checked: false,
    unchecked_fields: ["issued_at"],
    document_count: 1,
    source_count: 1,
    why: "Here because: flight booking with schema.org markup (unchecked).",
    origin: "schema",
    thread_id: "thread_tap",
    message_id: "msg_tap",
    dismissed: false,
    pdf: {
      message_id: "msg_tap",
      attachment_id: "att_tap",
      filename: "e-ticket-K7QX2M.pdf",
      mime_type: "application/pdf",
      size_bytes: 92_000,
      is_pdf: true,
      on_disk: false,
    },
    fields: [
      {
        field: "reference",
        label: "Booking ref",
        value: "K7QX2M",
        copy: "K7QX2M",
        source: "schema",
        source_label: "schema.org markup",
        checked: true,
      },
      {
        field: "amount",
        label: "Price",
        value: "£212.40",
        copy: "212.40",
        source: "schema",
        source_label: "schema.org markup",
        checked: true,
      },
      {
        field: "issued_at",
        label: "Date",
        value: "2 Jun 2025",
        copy: "2 Jun 2025",
        source: "rule",
        source_label: "a pattern in the email",
        checked: false,
        evidence: "the email's date",
      },
    ],
    ...overrides,
  };
}

export function monthFixture(month: string, count: number, total?: string): RecordMonth {
  const [year, number] = month.split("-");
  const names = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
  ];
  return {
    month,
    label: `${year} · ${names[Number(number) - 1]}`,
    count,
    totals: total ? [{ minor: 1, currency: "GBP", display: total }] : [],
  };
}

export function answerFixture(overrides: Partial<RecordAnswer> = {}): RecordAnswer {
  const record = recordFixture({
    group: { id: "grp", kind: "trip", title: "Lisbon, June 2025", count: 3 },
  });
  return {
    query: "lisbon booking ref",
    asked: "reference",
    answer: {
      record,
      field: "reference",
      label: "Booking ref",
      value: "K7QX2M",
      copy: "K7QX2M",
      provenance: record.fields?.[0],
    },
    also: [recordFixture({ id: "rec_hotel", title: "Lisbon, 3 nights", reference: "88213" })],
    mode: "answer",
    matching: 2,
    ...overrides,
  };
}

/** "anthropic": three invoices from one issuer, listed, the April one best. */
export function listFixture(overrides: Partial<RecordAnswerList> = {}): RecordAnswerList {
  const invoice = (id: string, date: string, display: string) =>
    recordFixture({
      id,
      kind: "invoice",
      kind_label: "Invoice",
      issuer: "Anthropic",
      title: "Claude Pro",
      reference: null,
      date,
      amount: { minor: 1_800, currency: "GBP", display },
    });
  return {
    header: "Anthropic · 3 records · £54.00",
    count: 3,
    offset: 0,
    records: [
      invoice("rec_may", "2025-05-03T12:00:00Z", "£18.00"),
      invoice("rec_apr", "2025-04-03T12:00:00Z", "£18.00"),
      invoice("rec_mar", "2025-03-03T12:00:00Z", "£18.00"),
    ],
    months: [
      monthFixture("2025-05", 1, "£18.00"),
      monthFixture("2025-04", 1, "£18.00"),
      monthFixture("2025-03", 1, "£18.00"),
    ],
    totals: [{ minor: 5_400, currency: "GBP", display: "£54.00" }],
    first: "2025-03-03T12:00:00Z",
    last: "2025-05-03T12:00:00Z",
    top_record_id: "rec_apr",
    issuer: "Anthropic",
    ...overrides,
  };
}

export function subscriptionFixture(
  overrides: Partial<RecordSubscription> = {},
): RecordSubscription {
  return {
    id: "sub_netflix",
    account_id: "acct",
    issuer: "Netflix",
    title: "Netflix",
    cadence: "monthly",
    cadence_label: "Monthly",
    amount: { minor: 1299, currency: "GBP", display: "£12.99" },
    yearly_cost: { minor: 15_588, currency: "GBP", display: "£155.88" },
    start: "2025-01-10T12:00:00Z",
    last_charge: "2025-04-10T12:00:00Z",
    next_expected: "2025-05-10T12:00:00Z",
    status: "active",
    status_reason: "4 charges about a month apart since Jan 2025",
    confirmed: true,
    charge_count: 4,
    charges: [
      {
        record_ids: ["rec_jan"],
        date: "2025-01-10T12:00:00Z",
        amount: { minor: 1099, currency: "GBP", display: "£10.99" },
        checked: false,
      },
      {
        record_ids: ["rec_apr"],
        date: "2025-04-10T12:00:00Z",
        amount: { minor: 1299, currency: "GBP", display: "£12.99" },
        checked: false,
      },
    ],
    price_changes: [
      {
        date: "2025-04-10T12:00:00Z",
        record_id: "rec_apr",
        from: { minor: 1099, currency: "GBP", display: "£10.99" },
        to: { minor: 1299, currency: "GBP", display: "£12.99" },
        label: "£10.99 to £12.99 on 10 Apr 2025",
      },
    ],
    fields: [
      {
        field: "amount",
        label: "Amount",
        value: "£12.99",
        copy: "12.99",
        source: "rule",
        source_label: "a pattern in the email",
        checked: false,
      },
      {
        field: "cadence",
        label: "Every",
        value: "a month",
        copy: "a month",
        source: "derived",
        source_label: "your 4 charges",
        checked: false,
      },
    ],
    one_offs: 0,
    record_id: "rec_apr",
    thread_id: "thread_netflix",
    why: "Here because: 4 charges from Netflix about a month apart (worked out from your records).",
    ...overrides,
  };
}

export function subscriptionsFixture(
  overrides: Partial<RecordSubscriptions> = {},
): RecordSubscriptions {
  return {
    header:
      "Receipts that come every week, month, quarter or year, with the next charge and what they cost.",
    subscriptions: [
      subscriptionFixture(),
      subscriptionFixture({
        id: "sub_disney",
        title: "Disney+",
        issuer: "Disney+",
        status: "ended",
        next_expected: null,
      }),
    ],
    totals: [
      {
        currency: "GBP",
        per_month: { minor: 1299, currency: "GBP", display: "£12.99" },
        per_year: { minor: 15_588, currency: "GBP", display: "£155.88" },
      },
    ],
    signals: [
      {
        kind: "price_change",
        subscription_id: "sub_netflix",
        record_id: "rec_apr",
        at: "2025-04-10T12:00:00Z",
        label: "Netflix went up from £10.99 to £12.99 on 10 Apr",
      },
    ],
    live: 1,
    ended: 1,
    ...overrides,
  };
}
