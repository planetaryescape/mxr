import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { UpdateLine } from "./api";
import { lineFixture } from "./testing";
import { UpdateLineRow } from "./UpdateLineRow";

function renderLine(line: UpdateLine) {
  const handlers = {
    onSelect: vi.fn<(line: UpdateLine) => void>(),
    onLetGo: vi.fn<(line: UpdateLine) => void>(),
    onNeedsMe: vi.fn<(line: UpdateLine) => void>(),
    onTune: vi.fn<(line: UpdateLine) => void>(),
    onOpenEmail: vi.fn<(line: UpdateLine) => void>(),
    onTuneTo: vi.fn<(line: UpdateLine, setting: "muted" | "changes_only") => void>(),
    onSweep: vi.fn<(line: UpdateLine) => void>(),
    onPin: vi.fn<(line: UpdateLine) => void>(),
  };
  render(
    <ul>
      <UpdateLineRow line={line} index={0} focused {...handlers} />
    </ul>,
  );
  return { handlers, row: screen.getByTestId("update-line") };
}

describe("UpdateLineRow", () => {
  test("reads as a source and its fact, with the computed delta and why", () => {
    const { row, handlers } = renderLine(lineFixture());
    const view = within(row);
    expect(view.getByTestId("update-source")).toHaveTextContent("Strava");
    expect(view.getByTestId("update-fact")).toHaveTextContent("21.3 km over 3 runs");
    expect(view.getByTestId("update-delta")).toHaveTextContent("up 12% on last week");
    expect(view.getByTestId("update-delta")).toHaveAttribute(
      "title",
      "Computed from 21.3 km and 19.0 km",
    );
    expect(view.getByTestId("update-why")).toHaveTextContent("In the 08:00 digest.");
    expect(view.getByRole("link", { name: "Open strava.com" })).toHaveAttribute(
      "href",
      "https://www.strava.com/athlete/training",
    );
    fireEvent.click(view.getByRole("button", { name: "This needs me" }));
    expect(handlers.onNeedsMe).toHaveBeenCalledTimes(1);
    fireEvent.click(view.getByRole("button", { name: "Let go" }));
    expect(handlers.onLetGo).toHaveBeenCalledTimes(1);
  });

  test("a sign-in already in To do says so and opens no link", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        signal: "needs_you",
        source_name: "Google",
        fact: "Security alert: New sign-in",
        delta: undefined,
        link: { url: "https://myaccount.google.com/", domain: "google.com" },
        in_todo: "already in To do",
        todo_id: "todo_1",
        time_label: "06:12",
      }),
    );
    const view = within(row);
    expect(view.getByTestId("update-in-todo")).toHaveTextContent("already in To do");
    expect(view.queryByRole("link")).toBeNull();
    expect(view.queryByRole("button", { name: "This needs me" })).toBeNull();
    expect(row).toHaveTextContent("06:12");
  });

  test("a parcel leads with status and identity, with full references in keyboard-accessible details", () => {
    const { row, handlers } = renderLine(
      lineFixture({
        source_key: "parcel:1",
        source_name: "Bookshop",
        fact: "Parcel out for delivery",
        latest_thread_id: "thread-1",
        message_ids: [],
        delta: undefined,
        link: undefined,
        latest_message_id: undefined,
        tracker: {
          kind: "parcel",
          state: "out_for_delivery",
          state_label: "out for delivery",
          outcome: "progress",
          steps: ["ordered", "shipped", "out for delivery", "delivered"],
          step: 2,
          detail: "Expected by Thu 8 Oct · DHL",
          items: [
            { name: "Blue notebook", quantity: 1 },
            { name: "Pen set", quantity: 2 },
          ],
          tracking_number: "A-VERY-LONG-TRACKING-REFERENCE-1234567890",
          order_number: "ORDER-987",
        },
      }),
    );
    const view = within(row);
    expect(view.queryByTestId("parcel-track")).toBeNull();
    expect(row).toHaveTextContent("Expected by Thu 8 Oct · DHL");
    expect(view.getByTestId("parcel-identity")).toHaveTextContent("Blue notebook +1 more");
    expect(view.getByTestId("parcel-identity")).toHaveTextContent(
      "A-VERY-LONG-TRACKING-REFERENCE-1234567890",
    );
    expect(view.getByTestId("update-why").closest("details")).not.toHaveAttribute("open");
    const details = view.getByText("Details");
    expect(details).toHaveAttribute(
      "aria-label",
      "Details for Bookshop parcel with reference A-VERY-LONG-TRACKING-REFERENCE-1234567890",
    );
    expect(details.parentElement).toHaveClass("open:basis-full");
    expect(details.parentElement?.parentElement).toContainElement(
      view.getByRole("button", { name: "From email" }),
    );
    details.focus();
    expect(handlers.onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ source_name: "Bookshop" }),
    );
    fireEvent.click(details);
    expect(view.getByTestId("update-why")).toHaveTextContent("Here because");
    expect(view.getByRole("list", { name: "Available parcel items" })).toHaveTextContent(
      "Pen set · quantity 2",
    );
    expect(view.getByText("Order reference: ORDER-987")).toBeInTheDocument();
    fireEvent.click(view.getByRole("button", { name: "From email" }));
    expect(handlers.onOpenEmail).toHaveBeenCalledTimes(1);
    expect(view.queryByRole("button", { name: "Let go" })).toBeNull();
    expect(view.queryByRole("button", { name: "Tune" })).toBeNull();
  });

  test("parcel identity falls back cleanly for an old payload", () => {
    const oldPayload = lineFixture({
      source_name: "Bookshop",
      fact: "Parcel on its way",
      tracker: {
        kind: "parcel",
        state: "in_transit",
        state_label: "on its way",
        outcome: "progress",
      },
    });
    const { row } = renderLine(oldPayload);
    expect(within(row).queryByTestId("parcel-identity")).toBeNull();
    expect(within(row).getByTestId("update-fact")).toHaveTextContent("Parcel on its way");
  });

  test("same-merchant parcels remain distinguishable by their full references", () => {
    const first = lineFixture({
      id: "parcel-1",
      source_name: "Bookshop",
      source_key: "parcel:1",
      fact: "Parcel on its way",
      tracker: {
        kind: "parcel",
        state: "in_transit",
        state_label: "on its way",
        outcome: "progress",
        tracking_number: "TRACK-12345678901234567890-A",
      },
    });
    const second = lineFixture({
      ...first,
      id: "parcel-2",
      source_key: "parcel:2",
      tracker: { ...first.tracker!, tracking_number: "TRACK-12345678901234567890-B" },
    });
    const handlers = {
      onSelect: vi.fn<(line: UpdateLine) => void>(),
      onLetGo: vi.fn<(line: UpdateLine) => void>(),
      onNeedsMe: vi.fn<(line: UpdateLine) => void>(),
      onTune: vi.fn<(line: UpdateLine) => void>(),
      onOpenEmail: vi.fn<(line: UpdateLine) => void>(),
      onTuneTo: vi.fn<(line: UpdateLine, setting: "muted" | "changes_only") => void>(),
      onSweep: vi.fn<(line: UpdateLine) => void>(),
      onPin: vi.fn<(line: UpdateLine) => void>(),
    };
    render(
      <ul>
        {[first, second].map((line, index) => (
          <UpdateLineRow key={line.id} line={line} index={index} focused={false} {...handlers} />
        ))}
      </ul>,
    );
    const identities = screen.getAllByTestId("parcel-identity");
    expect(identities).toHaveLength(2);
    expect(identities[0]).toHaveTextContent("TRACK-12345678901234567890-A");
    expect(identities[1]).toHaveTextContent("TRACK-12345678901234567890-B");
  });

  test.each([
    ["available_for_pickup", "ready to collect"],
    ["attempt_fail", "delivery attempt failed"],
    ["exception", "delivery problem"],
  ])("a %s parcel keeps its action clear in text", (state, label) => {
    const { row } = renderLine(lineFixture({
      fact: `Parcel ${label.toLowerCase()}`,
      tracker: {
        kind: "parcel",
        state,
        state_label: label.toLowerCase(),
        outcome: state === "available_for_pickup" ? "progress" : "bad",
      },
    }));
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(label);
  });

  test("a delivered parcel keeps a quiet fact and does not repeat the state", () => {
    const { row } = renderLine(lineFixture({
      fact: "Parcel delivered",
      tracker: {
        kind: "parcel",
        state: "delivered",
        state_label: "delivered",
        outcome: "good",
      },
    }));
    const fact = within(row).getByTestId("update-fact");
    expect(fact).toHaveTextContent("Parcel delivered");
    expect(fact).toHaveClass("text-muted-foreground");
    expect(row.querySelector("svg")).toHaveClass("text-muted-foreground/50");
    expect(within(row).queryByText("delivered ·", { exact: false })).toBeNull();
  });

  test("a source's sender can be swept and its latest email pinned", () => {
    const { row, handlers } = renderLine(lineFixture());
    const view = within(row);
    fireEvent.click(view.getByRole("button", { name: "Sweep sender" }));
    expect(handlers.onSweep).toHaveBeenCalledTimes(1);
    fireEvent.click(view.getByRole("button", { name: "Pin" }));
    expect(handlers.onPin).toHaveBeenCalledTimes(1);
  });

  test("a parcel has no sender's mail to sweep", () => {
    const { row } = renderLine(
      lineFixture({
        sender_email: undefined,
        latest_message_id: undefined,
        tracker: {
          kind: "parcel",
          state: "shipped",
          state_label: "shipped",
          outcome: "progress",
          steps: ["ordered", "shipped"],
          step: 1,
          detail: "DHL",
        },
      }),
    );
    expect(within(row).queryByRole("button", { name: "Sweep sender" })).toBeNull();
    expect(within(row).queryByRole("button", { name: "Pin" })).toBeNull();
  });

  test("the mute question offers its two answers", () => {
    const { row, handlers } = renderLine(
      lineFixture({
        suggestion:
          "You've let go of Strava 8 digests in a row without opening it. Mute it, or changes only?",
      }),
    );
    fireEvent.click(within(row).getByRole("button", { name: "Changes only" }));
    expect(handlers.onTuneTo).toHaveBeenCalledWith(expect.anything(), "changes_only");
  });
});
