import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterAll, beforeEach, describe, expect, test, vi } from "vitest";

import type { ScreenerQuestion } from "@/features/modes/membership";

import { MoveToModeDialog, newSenderChoice, pickerChoice } from "./MoveToModeDialog";

const moves = vi.hoisted(() => ({ performMove: vi.fn<(...args: unknown[]) => void>() }));
const correct = vi.hoisted(() => vi.fn<(...args: unknown[]) => Promise<boolean>>());
const realSetTimeout = globalThis.setTimeout;
vi.mock("./moves", () => moves);
vi.mock("@/features/places/senderKind", () => ({ correctSender: correct }));

const question: ScreenerQuestion = {
  account_id: "acct",
  sender_email: "noor@example.com",
  question: "New sender. Keep in Messages?",
  choices: [],
};

beforeEach(() => {
  moves.performMove.mockReset();
  correct.mockReset().mockResolvedValue(true);
});

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
    // Five letters for this email, then the three capitals for the sender.
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

  test("the highlight starts on the first option; j, k and the arrows move it", async () => {
    render(
      <MoveToModeDialog
        subject={{ messageId: "msg-1", label: "Q4 plan" }}
        sender={false}
        onClose={() => {}}
      />,
    );
    const messages = screen.getByTestId("move-choice-messages");
    await waitFor(() => expect(messages).toHaveFocus());
    fireEvent.keyDown(messages, { key: "j" });
    expect(screen.getByTestId("move-choice-todo")).toHaveFocus();
    fireEvent.keyDown(screen.getByTestId("move-choice-todo"), { key: "ArrowUp" });
    expect(messages).toHaveFocus();
  });

  test("a Not-sure email starts on where it is now, and a key answers the question", async () => {
    const answer = vi.fn<(mode: string, sender: boolean) => void>();
    render(
      <MoveToModeDialog
        subject={{
          messageId: "msg-2",
          label: "Invoice",
          notSure: { mode: "updates", answer },
        }}
        sender={false}
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(screen.getByTestId("move-choice-updates")).toHaveFocus());
    fireEvent.keyDown(screen.getByTestId("move-to-mode-dialog"), { key: "x" });
    expect(answer).toHaveBeenCalledWith("todo", false);
    expect(moves.performMove).not.toHaveBeenCalled();
  });

  test("on a new sender's row the capitals answer their question; D blocks", () => {
    expect(newSenderChoice("M", false)).toBe("people");
    expect(newSenderChoice("U", false)).toBe("paper_trail");
    expect(newSenderChoice("D", false)).toBe("screened_out");
    // Lowercase still moves the email in X's picker; P's picker takes either case.
    expect(newSenderChoice("m", false)).toBeNull();
    expect(newSenderChoice("d", true)).toBe("screened_out");

    const onClose = vi.fn<() => void>();
    render(
      <MoveToModeDialog
        subject={{
          messageId: "msg-3",
          label: "Hello",
          senderLabel: "Noor",
          newSender: { question, label: "Noor" },
        }}
        sender={false}
        onClose={onClose}
      />,
    );
    expect(screen.getByTestId("new-sender-choice-screened_out")).toHaveTextContent("D");
    fireEvent.keyDown(screen.getByTestId("move-to-mode-dialog"), { key: "D" });
    expect(correct).toHaveBeenCalledExactlyOnceWith(
      { accountId: "acct", senderEmail: "noor@example.com", label: "Noor" },
      "screened_out",
    );
    expect(moves.performMove).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  test("P on a new sender's row is their question alone", () => {
    render(
      <MoveToModeDialog
        subject={{ messageId: "msg-4", label: "Hello", newSender: { question, label: "Noor" } }}
        sender
        onClose={() => {}}
      />,
    );
    expect(screen.queryByTestId("move-choice-messages")).toBeNull();
    fireEvent.keyDown(screen.getByTestId("move-to-mode-dialog"), { key: "r" });
    expect(correct).toHaveBeenCalledWith(expect.anything(), "reading");
  });
});
