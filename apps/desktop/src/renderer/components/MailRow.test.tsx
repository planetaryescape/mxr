import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MailRow, MailRowSkeleton, DateGroupHeader } from "./MailRow";

function makeRow(overrides: Partial<Parameters<typeof MailRow>[0]["row"]> = {}) {
  return {
    id: "msg-1",
    thread_id: "thread-1",
    provider_id: "gmail-msg-1",
    sender: "Alice Smith",
    sender_detail: "alice@example.com",
    subject: "Weekly standup notes",
    snippet: "Here are the notes from today's standup meeting.",
    date_label: "2h",
    unread: false,
    starred: false,
    has_attachments: false,
    ...overrides,
  };
}

const noop = () => {};

describe("MailRow", () => {
  it("renders sender, subject, snippet, and date", () => {
    render(
      <MailRow
        row={makeRow()}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    expect(screen.getByText("Alice Smith")).toBeInTheDocument();
    expect(screen.getByText("Weekly standup notes")).toBeInTheDocument();
    expect(screen.getByText(/notes from today/)).toBeInTheDocument();
    expect(screen.getByText("2h")).toBeInTheDocument();
  });

  it("shows unread dot when message is unread", () => {
    const { container } = render(
      <MailRow
        row={makeRow({ unread: true })}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    const dot = container.querySelector(".bg-accent.rounded-full");
    expect(dot).not.toBeNull();
  });

  it("hides unread dot when message is read", () => {
    const { container } = render(
      <MailRow
        row={makeRow({ unread: false })}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    const dot = container.querySelector(".bg-accent.rounded-full");
    expect(dot).toBeNull();
  });

  it("renders star icon when starred", () => {
    render(
      <MailRow
        row={makeRow({ starred: true })}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    expect(document.querySelector(".fill-warning")).not.toBeNull();
  });

  it("does not render star icon when not starred", () => {
    render(
      <MailRow
        row={makeRow({ starred: false })}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    expect(document.querySelector(".fill-warning")).toBeNull();
  });

  it("shows Syncing label when pending", () => {
    render(
      <MailRow
        row={makeRow()}
        selected={false}
        multiSelected={false}
        pending={true}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    expect(screen.getByText("Syncing")).toBeInTheDocument();
    expect(screen.queryByText("2h")).not.toBeInTheDocument();
  });

  it("shows date when not pending", () => {
    render(
      <MailRow
        row={makeRow()}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    expect(screen.getByText("2h")).toBeInTheDocument();
    expect(screen.queryByText("Syncing")).not.toBeInTheDocument();
  });

  it("applies accent border when selected", () => {
    render(
      <MailRow
        row={makeRow()}
        selected={true}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    const button = screen.getByRole("button");
    expect(button.className).toContain("border-l-accent");
    expect(button.className).toContain("bg-panel-elevated");
  });

  it("applies success border when multi-selected", () => {
    render(
      <MailRow
        row={makeRow()}
        selected={false}
        multiSelected={true}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    const button = screen.getByRole("button");
    expect(button.className).toContain("border-l-success");
  });

  it("applies exit animation class when removing", () => {
    render(
      <MailRow
        row={makeRow()}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={true}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    const button = screen.getByRole("button");
    expect(button.className).toContain("row-exit");
  });

  it("calls onSelect on click and onOpen on double click", () => {
    const onSelect = vi.fn();
    const onOpen = vi.fn();

    render(
      <MailRow
        row={makeRow()}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={onSelect}
        onOpen={onOpen}
        onContextMenu={noop}
      />,
    );

    fireEvent.click(screen.getByRole("button"));
    expect(onSelect).toHaveBeenCalledTimes(1);

    fireEvent.dblClick(screen.getByRole("button"));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("renders attachment icon when has_attachments is true", () => {
    render(
      <MailRow
        row={makeRow({ has_attachments: true })}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    // Paperclip icon from lucide renders an SVG
    expect(document.querySelector("svg.shrink-0")).not.toBeNull();
  });

  it("bolds sender text when unread", () => {
    render(
      <MailRow
        row={makeRow({ unread: true })}
        selected={false}
        multiSelected={false}
        pending={false}
        removing={false}
        onSelect={noop}
        onOpen={noop}
        onContextMenu={noop}
      />,
    );

    const sender = screen.getByText("Alice Smith");
    expect(sender.className).toContain("font-semibold");
  });
});

describe("MailRowSkeleton", () => {
  it("renders skeleton elements", () => {
    const { container } = render(<MailRowSkeleton />);
    expect(container.querySelectorAll(".skeleton").length).toBeGreaterThan(0);
  });
});

describe("DateGroupHeader", () => {
  it("renders the label text", () => {
    render(<DateGroupHeader label="Today" />);
    expect(screen.getByText("Today")).toBeInTheDocument();
  });
});
