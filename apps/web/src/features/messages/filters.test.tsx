import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { filterOf, isFiltered, NO_FILTER, searchOf, type MessagesFilter } from "./filters";
import { MessagesFilterBar } from "./MessagesFilterBar";

describe("Messages attention and history views", () => {
  it("requires an explicit history choice and clearing restores attention", () => {
    const onChange = vi.fn<(next: MessagesFilter) => void>();
    const { rerender } = render(
      <MessagesFilterBar
        filter={NO_FILTER}
        counts={{ matched: 0, total: 0 }}
        onChange={onChange}
      />,
    );
    expect(screen.getByRole("radio", { name: "Needs attention" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "All conversations" }));
    expect(onChange).toHaveBeenLastCalledWith({ ...NO_FILTER, turn: "all" });

    rerender(
      <MessagesFilterBar
        filter={{ ...NO_FILTER, turn: "all" }}
        counts={{ matched: 29, total: 0 }}
        onChange={onChange}
      />,
    );
    expect(screen.getByText("29 shown · 0 need attention")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(onChange).toHaveBeenLastCalledWith(NO_FILTER);
  });

  it("keeps an omitted view distinct from All in URLs and filters", () => {
    expect(filterOf({})).toEqual(NO_FILTER);
    expect(isFiltered(NO_FILTER)).toBe(false);
    const history = filterOf({ turn: "all", unread: true });
    expect(history).toEqual({ turn: "all", unread: true, pastPace: false });
    expect(searchOf(history)).toEqual({ turn: "all", unread: true });
    expect(searchOf(NO_FILTER)).toEqual({});
    expect(isFiltered(history)).toBe(true);
  });
});
