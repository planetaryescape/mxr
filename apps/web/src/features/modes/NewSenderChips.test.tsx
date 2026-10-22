/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { definedShortcut } from "@/lib/actions/definition";

import type { ScreenerQuestion } from "./membership";
import {
  NEW_SENDER_NOTE,
  NewSenderChips,
  newSenderCommands,
  SENDER_CHOICES,
} from "./NewSenderChips";

const correct = vi.hoisted(() => vi.fn<(...args: unknown[]) => Promise<boolean>>());
vi.mock("@/features/places/senderKind", () => ({ correctSender: correct }));

const question: ScreenerQuestion = {
  account_id: "acct",
  sender_email: "uma@example.com",
  question: "New sender. Keep in Messages?",
  choices: [],
};

beforeEach(() => correct.mockReset().mockResolvedValue(true));

describe("new sender choice", () => {
  test("four chips with keys 1 to 4, in a fixed order, named in their tooltips", () => {
    render(<NewSenderChips question={question} label="Uma" />);
    expect(screen.getAllByRole("button").map((button) => button.getAttribute("title"))).toEqual([
      "Messages (1)",
      "Updates (2)",
      "Reading (3)",
      "Block (4)",
    ]);
    expect(NEW_SENDER_NOTE).toBe("New sender: 1 Messages · 2 Updates · 3 Reading · 4 Block");
  });

  test("Messages answers on Now's keys, since the same chips show in both", () => {
    for (const { slot, key } of SENDER_CHOICES) {
      expect(definedShortcut(`messages.sender-${slot}`)).toBe(key);
    }
  });

  test("a chip answers with that sender kind, once, without lighting the row under it", () => {
    const onRow = vi.fn<() => void>();
    render(
      // oxlint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions
      <div onClick={onRow}>
        <NewSenderChips question={question} label="Uma" />
      </div>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Block" }));
    expect(correct).toHaveBeenCalledExactlyOnceWith(
      { accountId: "acct", senderEmail: "uma@example.com", label: "Uma" },
      "screened_out",
    );
    expect(onRow).not.toHaveBeenCalled();
  });

  test("the key commands exist only while a new sender is lit, so the digits stay tabs elsewhere", () => {
    expect(newSenderCommands(null, "Uma")).toEqual({});
    expect(newSenderCommands(undefined, "Uma")).toEqual({});
    const commands = newSenderCommands(question, "Uma");
    expect(Object.keys(commands)).toEqual(SENDER_CHOICES.map(({ slot }) => `senderChoice${slot}`));
    commands.senderChoice3?.();
    expect(correct).toHaveBeenCalledWith(expect.anything(), "reading");
  });
});
