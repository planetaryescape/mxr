import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useKeyScope } from "@/state/keyScopeStore";

import { HelpDialog } from "./HelpDialog";

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useRouterState: ({
    select,
  }: {
    select: (state: { location: { pathname: string } }) => unknown;
  }) => select({ location: { pathname: "/m/inbox" } }),
}));

function headings(): string[] {
  // Section headings; the dialog title is a heading too.
  return screen
    .getAllByRole("heading", { level: 2 })
    .map((heading) => heading.textContent ?? "")
    .filter((text) => text !== "Keyboard");
}

function filter(value: string) {
  fireEvent.change(screen.getByLabelText("Filter shortcuts"), { target: { value } });
}

beforeEach(() => {
  useKeyScope.setState({ stack: ["list"], pendingPrefix: null });
});

afterEach(() => {
  useKeyScope.setState({ stack: [], pendingPrefix: null });
});

describe("HelpDialog", () => {
  test("lists the current view's keys first, then mail actions and global keys", () => {
    render(<HelpDialog open onOpenChange={() => undefined} />);

    expect(screen.getByRole("dialog", { name: "Keyboard" })).toBeVisible();
    expect(headings().slice(0, 5)).toEqual([
      "Mail list (this view)",
      "Mail actions",
      "Everywhere",
      "Search",
      "Go to",
    ]);
    expect(screen.getByText("Select and move down")).toBeVisible();
    expect(screen.getByText("Go to Inbox")).toBeVisible();
    expect(screen.getByText("Keyboard help")).toBeVisible();
  });

  test("shows the reader first while reading", () => {
    useKeyScope.setState({ stack: ["list", "reader"], pendingPrefix: null });
    render(<HelpDialog open onOpenChange={() => undefined} />);

    expect(headings()[0]).toBe("Reader (this view)");
  });

  test("filters by label, key or TUI note, dropping empty sections", () => {
    render(<HelpDialog open onOpenChange={() => undefined} />);

    filter("archive");
    expect(screen.getByText("Mark read and archive")).toBeVisible();
    expect(screen.queryByText("Go to Inbox")).not.toBeInTheDocument();
    expect(headings()).not.toContain("Go to");

    filter("g i");
    expect(screen.getByText("Go to Inbox")).toBeVisible();

    filter("gmail muscle memory");
    expect(screen.getByText("Undo last action")).toBeVisible();
  });

  test("says so when nothing matches", () => {
    render(<HelpDialog open onOpenChange={() => undefined} />);

    filter("no-such-shortcut");

    expect(screen.getByText("No matching shortcuts.")).toBeVisible();
  });

  test("clears the filter when closed", () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    const { rerender } = render(<HelpDialog open onOpenChange={onOpenChange} />);
    filter("archive");

    fireEvent.keyDown(screen.getByLabelText("Filter shortcuts"), { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
    rerender(<HelpDialog open onOpenChange={onOpenChange} />);

    expect(screen.getByLabelText("Filter shortcuts")).toHaveValue("");
  });
});
