import { fireEvent, render, screen } from "@testing-library/react";
import { afterAll, describe, expect, test, vi } from "vitest";

import { MoveToModeDialog, pickerChoice } from "./MoveToModeDialog";

const moves = vi.hoisted(() => ({ performMove: vi.fn<(...args: unknown[]) => void>() }));
const realSetTimeout = globalThis.setTimeout;
vi.mock("./moves", () => moves);

// Drain Radix's focus-scope unmount timer before Vitest tears down jsdom globals.
afterAll(() => new Promise<void>((resolve) => realSetTimeout(resolve, 0)));

describe("the move picker's keys", () => {
  test("letters move this email; capitals send the sender's mail", () => {
    expect(pickerChoice("r", false)).toEqual({ mode: "reading", sender: false });
    expect(pickerChoice("x", false)).toEqual({ mode: "todo", sender: false });
    expect(pickerChoice("e", false)).toEqual({ mode: "archive", sender: false });
    expect(pickerChoice("R", false)).toEqual({ mode: "reading", sender: true });
    // A sender's mail can't all go to To do or Archive.
    expect(pickerChoice("X", false)).toBeNull();
    expect(pickerChoice("e", true)).toBeNull();
    expect(pickerChoice("m", true)).toEqual({ mode: "messages", sender: true });
    expect(pickerChoice("q", false)).toBeNull();
  });

  test("a key in the dialog moves the email and closes it", () => {
    const onClose = vi.fn<(...args: unknown[]) => void>();
    render(
      <MoveToModeDialog
        subject={{ messageId: "msg-1", label: "Q4 plan", senderLabel: "Maya" }}
        sender={false}
        onClose={onClose}
      />,
    );
    expect(
      screen.getAllByRole("button", { name: /Messages|To do|Updates|Reading|Archive/ }),
    ).toHaveLength(5);
    fireEvent.keyDown(screen.getByTestId("move-to-mode-dialog"), { key: "u" });
    expect(onClose).toHaveBeenCalled();
    expect(moves.performMove).toHaveBeenCalledWith({
      messageId: "msg-1",
      mode: "updates",
      sender: false,
    });
  });

  test("the sender picker offers only the modes a sender's mail can go to", () => {
    render(
      <MoveToModeDialog
        subject={{ messageId: "msg-1", label: "Q4 plan", senderLabel: "Maya" }}
        sender
        onClose={vi.fn<(...args: unknown[]) => void>()}
      />,
    );
    expect(screen.getByText("Send everything from Maya to…")).toBeVisible();
    expect(screen.queryByTestId("move-choice-todo")).toBeNull();
    expect(screen.queryByTestId("move-choice-archive")).toBeNull();
  });
});
