import { fireEvent, render, screen } from "@testing-library/react";
import { afterAll, beforeEach, describe, expect, test, vi } from "vitest";

import { MoveToModeDialog } from "./MoveToModeDialog";

const moves = vi.hoisted(() => ({ performMove: vi.fn<(...args: unknown[]) => void>() }));
const correct = vi.hoisted(() => vi.fn<(...args: unknown[]) => Promise<boolean>>());
const realSetTimeout = globalThis.setTimeout;
vi.mock("./moves", () => moves);
vi.mock("@/features/places/senderKind", () => ({ correctSender: correct }));

beforeEach(() => {
  moves.performMove.mockReset();
  correct.mockReset().mockResolvedValue(true);
});

// Drain Radix's focus-scope unmount timer before Vitest tears down jsdom globals.
afterAll(() => new Promise<void>((resolve) => realSetTimeout(resolve, 0)));

describe("the move picker's keys", () => {
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
    ).toHaveLength(8);
    expect(screen.getByTestId("move-sender-choice-reading")).toHaveTextContent("R");
    fireEvent.keyDown(screen.getByTestId("move-to-mode-dialog"), { key: "u" });
    expect(onClose).toHaveBeenCalled();
    expect(moves.performMove).toHaveBeenCalledWith({
      messageId: "msg-1",
      mode: "updates",
      sender: false,
    });
  });
});
