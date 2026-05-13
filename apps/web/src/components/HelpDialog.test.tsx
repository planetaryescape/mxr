/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { HelpDialog } from "./HelpDialog";

describe("HelpDialog", () => {
  test("shows contextual shortcuts and filters them by search", () => {
    render(
      <HelpDialog
        open
        onOpenChange={vi.fn<(open: boolean) => void>()}
        path="/m/inbox/thread-1"
        activePane="reader"
      />,
    );

    expect(screen.getByRole("dialog", { name: /help/i })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Reader shortcuts" })).toBeVisible();
    expect(screen.getByText("Reply all")).toBeVisible();

    fireEvent.change(screen.getByLabelText(/search help/i), { target: { value: "trash" } });

    expect(screen.getByText("Trash")).toBeVisible();
    expect(screen.queryByText("Reply all")).not.toBeInTheDocument();
  });
});
