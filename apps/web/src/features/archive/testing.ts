/* Records shaped like the daemon's, for Archive's unit tests. */

import type { RecordAnswer, RecordData, RecordMonth } from "./api";

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
    ...overrides,
  };
}
