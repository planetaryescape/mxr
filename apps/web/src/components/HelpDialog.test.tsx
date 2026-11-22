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
  return screen
    .getAllByRole("heading", { level: 2 })
    .map((heading) => heading.textContent ?? "")
    .filter((text) => text !== "Help and keyboard shortcuts");
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

    expect(screen.getByRole("dialog", { name: "Help and keyboard shortcuts" })).toBeVisible();
    expect(headings().slice(0, 5)).toEqual([
      "Mail list (this view)",
      "Mail actions",
      "Everywhere",
      "Search",
      "Go to",
    ]);
    expect(screen.getAllByText("Select and move down")[0]).toBeVisible();
    expect(screen.getByText("Go to Inbox")).toBeVisible();
    expect(screen.getByText("Keyboard help")).toBeVisible();
  });

  test("? closes help while the filter is empty and types into it otherwise", () => {
    const onOpenChange = vi.fn<(open: boolean) => void>();
    render(<HelpDialog open onOpenChange={onOpenChange} />);
    filter("arch");
    fireEvent.keyDown(screen.getByLabelText("Filter shortcuts"), { key: "?" });
    expect(onOpenChange).not.toHaveBeenCalled();
    filter("");
    fireEvent.keyDown(screen.getByLabelText("Filter shortcuts"), { key: "?" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
