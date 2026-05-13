/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { HelpDialog } from "./HelpDialog";

describe("HelpDialog", () => {
  test("shows contextual page shortcuts and filters them by search", () => {
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

  test("renders global registry-derived navigation hints", () => {
    render(
      <HelpDialog
        open
        onOpenChange={vi.fn<(open: boolean) => void>()}
        path="/m/inbox"
        activePane="mailbox"
      />,
    );

    expect(screen.getByRole("heading", { name: "Navigation" })).toBeVisible();
    expect(screen.getByText("Go to Inbox")).toBeVisible();
    // "Compose" appears as both a section heading and an action label, so use
    // the heading-specific query to disambiguate.
    expect(screen.getByRole("heading", { name: "Compose" })).toBeVisible();
  });

  test("filter narrows hints by label substring", () => {
    render(
      <HelpDialog
        open
        onOpenChange={vi.fn<(open: boolean) => void>()}
        path="/m/inbox"
        activePane="mailbox"
      />,
    );

    fireEvent.change(screen.getByLabelText(/search help/i), {
      target: { value: "starred" },
    });

    expect(screen.getByText("Go to Starred")).toBeVisible();
    expect(screen.queryByText("Go to Inbox")).not.toBeInTheDocument();
  });
});
