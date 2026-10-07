import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { RecordData, RecordField } from "./api";
import { AnswerCard, cardFields, LedgerRow, RecordCard } from "./RecordParts";
import { answerFixture, recordFixture } from "./testing";

describe("Archive's cards and rows", () => {
  test("the answer card leads with the field asked for, its provenance and the trip", () => {
    const onCopy = vi.fn<(text: string) => void>();
    const onSelect = vi.fn<(record: RecordData) => void>();
    render(
      <AnswerCard
        answer={answerFixture()}
        onCopy={onCopy}
        onOpenDocument={vi.fn<(record: RecordData) => void>()}
        onOpenEmail={vi.fn<(record: RecordData) => void>()}
        onSelect={onSelect}
      />,
    );
    const card = screen.getByTestId("answer-card");
    expect(within(card).getByTestId("answer-value")).toHaveTextContent("K7QX2M");
    expect(card).toHaveTextContent("Booking ref");
    expect(card).toHaveTextContent("from schema.org markup · checked");
    expect(card).toHaveTextContent('Part of trip "Lisbon, June 2025" (3)');
    expect(card).toHaveTextContent("e-ticket-K7QX2M.pdf");
    fireEvent.click(within(card).getByRole("button", { name: /copy/ }));
    expect(onCopy).toHaveBeenCalledWith("K7QX2M");
    fireEvent.click(within(card).getByRole("button", { name: /Lisbon, 3 nights/ }));
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: "rec_hotel" }));
  });

  test("with no record match, the card says so before what all mail gave", () => {
    render(
      <AnswerCard
        answer={answerFixture({
          answer: null,
          also: [],
          fallback: {
            note: 'No record matches "boiler warranty". Searching all mail instead.',
            answer: {
              text: "The boiler was serviced in May.",
              citations: [
                {
                  message_id: "m1",
                  thread_id: "t1",
                  subject: "Boiler service",
                  date: "2025-05-01T09:00:00Z",
                  quote: "serviced",
                },
              ],
              retrieval: { requested_mode: "hybrid", executed_mode: "lexical", candidate_count: 1 },
            },
          },
        })}
        onCopy={vi.fn<(text: string) => void>()}
        onOpenDocument={vi.fn<(record: RecordData) => void>()}
        onOpenEmail={vi.fn<(record: RecordData) => void>()}
        onSelect={vi.fn<(record: RecordData) => void>()}
      />,
    );
    const fallback = screen.getByTestId("answer-fallback");
    expect(fallback).toHaveTextContent(
      'No record matches "boiler warranty". Searching all mail instead.',
    );
    expect(fallback).toHaveTextContent("Boiler service");
    expect(screen.queryByTestId("answer-card")).toBeNull();
  });

  test("a ledger row shows the fields you came for and marks an unchecked amount", () => {
    render(
      <ul>
        <LedgerRow
          record={recordFixture({ unchecked_fields: ["amount"], stage_line: "ordered · shipped" })}
          index={0}
          focused
          onSelect={vi.fn<(record: RecordData) => void>()}
          onOpen={vi.fn<(record: RecordData) => void>()}
        />
      </ul>,
    );
    const row = screen.getByTestId("record-row");
    expect(row).toHaveTextContent("TAP Air Portugal");
    expect(row).toHaveTextContent("£212.40");
    expect(row).toHaveTextContent("K7QX2M");
    expect(row).toHaveTextContent("ordered · shipped");
    expect(within(row).getByRole("img", { name: "amount unchecked" })).toBeInTheDocument();
    expect(row).toHaveAttribute("aria-current", "true");
  });

  test("the card lists the reference and amount first, each with where it came from", () => {
    const record = recordFixture();
    expect(cardFields(record).map((field) => field.field)).toEqual([
      "reference",
      "amount",
      "issued_at",
    ]);
    const onCopy = vi.fn<(field: RecordField) => void>();
    render(
      <RecordCard
        record={record}
        onCopy={onCopy}
        onOpenDocument={vi.fn<() => void>()}
        onOpenEmail={vi.fn<() => void>()}
        onIssuer={vi.fn<() => void>()}
      />,
    );
    const card = screen.getByTestId("record-card");
    expect(card).toHaveTextContent("from a pattern in the email · unchecked");
    expect(within(card).getAllByTestId("unchecked-dot")).toHaveLength(1);
    fireEvent.click(within(card).getByRole("button", { name: "Copy booking ref (y)" }));
    expect(onCopy).toHaveBeenCalledWith(expect.objectContaining({ field: "reference" }));
    expect(card).toHaveTextContent(record.why);
  });
});
