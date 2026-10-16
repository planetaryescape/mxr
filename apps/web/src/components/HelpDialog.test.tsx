import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { useState } from "react";

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
    .filter((text) => text !== "Help and keyboard shortcuts");
}

function filter(value: string) {
  fireEvent.change(screen.getByLabelText("Filter shortcuts"), { target: { value } });
}

function HelpDialogHarness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        First Help opener
      </button>
      <button type="button" onClick={() => setOpen(true)}>
        Second Help opener
      </button>
      <HelpDialog open={open} onOpenChange={setOpen} />
    </>
  );
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

    expect(screen.getByRole("dialog", { name: "Help and keyboard shortcuts" })).toBeVisible();
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

  test("filters by label or key, dropping empty sections", () => {
    render(<HelpDialog open onOpenChange={() => undefined} />);

    filter("read and archive");
    expect(screen.getByText("Mark read and archive")).toBeVisible();
    expect(screen.queryByText("Go to Inbox")).not.toBeInTheDocument();
    expect(headings()).not.toContain("Go to");

    filter("g i");
    expect(screen.getByText("Go to Inbox")).toBeVisible();

    // TUI-parity remarks live in the docs, not in the sheet.
    filter("gmail muscle memory");
    expect(screen.getByText("No matching shortcuts.")).toBeVisible();
  });

  test("states no TUI-parity remarks", () => {
    render(<HelpDialog open onOpenChange={() => undefined} />);
    expect(screen.queryByText(/Keys follow the mxr TUI/)).not.toBeInTheDocument();
    expect(screen.queryByText(/muscle memory/)).not.toBeInTheDocument();
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

  test("returns focus to the current opener after Escape and Close", async () => {
    render(<HelpDialogHarness />);
    const firstOpener = screen.getByRole("button", { name: "First Help opener" });
    const secondOpener = screen.getByRole("button", { name: "Second Help opener" });

    firstOpener.focus();
    fireEvent.click(firstOpener);
    await waitFor(() => expect(screen.getByLabelText("Filter shortcuts")).toHaveFocus());
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await waitFor(() => expect(firstOpener).toHaveFocus());

    secondOpener.focus();
    fireEvent.click(secondOpener);
    await waitFor(() => expect(screen.getByLabelText("Filter shortcuts")).toHaveFocus());
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(secondOpener).toHaveFocus());
  });

  test("returns keyboard-opened Help to the active pane", async () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    const { rerender } = render(
      <>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open={false} onOpenChange={onOpenChange} />
      </>,
    );
    const pane = screen.getByRole("main");
    pane.focus();
    rerender(
      <>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open onOpenChange={onOpenChange} />
      </>,
    );
    await waitFor(() => expect(screen.getByLabelText("Filter shortcuts")).toHaveFocus());

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    rerender(
      <>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open={false} onOpenChange={onOpenChange} />
      </>,
    );
    await waitFor(() => expect(pane).toHaveFocus());
  });

  test("falls back to the active pane when the opener is removed", async () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    const { rerender } = render(
      <>
        <button type="button">Help opener</button>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open={false} onOpenChange={onOpenChange} />
      </>,
    );
    const opener = screen.getByRole("button", { name: "Help opener" });
    opener.focus();
    rerender(
      <>
        <button type="button" disabled>
          Help opener
        </button>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open onOpenChange={onOpenChange} />
      </>,
    );
    rerender(
      <>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open onOpenChange={onOpenChange} />
      </>,
    );

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    rerender(
      <>
        <main data-active-pane="true" tabIndex={-1} />
        <HelpDialog open={false} onOpenChange={onOpenChange} />
      </>,
    );
    await waitFor(() => expect(document.querySelector("[data-active-pane='true']")).toHaveFocus());
  });
});
