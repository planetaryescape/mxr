/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { ScreenerQuestion } from "./membership";
import { NEW_SENDER_ANSWERS, NEW_SENDER_NOTE, NewSenderChips } from "./NewSenderChips";

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
  test("four chips in a fixed order, named in their tooltips with the X picker's capitals", () => {
    render(<NewSenderChips question={question} label="Uma" />);
    expect(screen.getAllByRole("button").map((button) => button.getAttribute("title"))).toEqual([
      "Messages (X then M)",
      "Updates (X then U)",
      "Reading (X then R)",
      "Block (X then D)",
    ]);
    expect(NEW_SENDER_NOTE).toBe("New sender: X then M Messages · U Updates · R Reading · D Block");
  });

  test("the answers are the g letters' capitals, as every picker of these places", () => {
    expect(NEW_SENDER_ANSWERS.map(({ kind, key }) => [kind, key])).toEqual([
      ["people", "M"],
      ["paper_trail", "U"],
      ["reading", "R"],
      ["screened_out", "D"],
    ]);
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
});
