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

  test("a parcel shows its track and offers no let go or tuning", () => {
    const { row } = renderLine(
      lineFixture({
        source_key: "parcel:1",
        source_name: "Bookshop",
        fact: "Parcel out for delivery",
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
          detail: "Arriving by Thu 8 Oct · DHL",
        },
      }),
    );
    const view = within(row);
    expect(view.getByTestId("parcel-track")).toHaveTextContent("out for delivery");
    expect(row).toHaveTextContent("Arriving by Thu 8 Oct · DHL");
    expect(view.queryByRole("button", { name: "Let go" })).toBeNull();
    expect(view.queryByRole("button", { name: "Tune" })).toBeNull();
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
