/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { AccountMark, SWATCH_CLASS } from "./AccountMark";
import { ACCOUNT_SLOTS } from "@/styles/accountSlots";

describe("AccountMark", () => {
  test("its accessible name is the account's name, and the tag is what shows", () => {
    render(
      <AccountMark identity={{ name: "Work", email: "bk@work.example", tag: "W", slot: "sky" }} />,
    );
    const chip = screen.getByRole("img", { name: "Work" });
    expect(chip).toHaveTextContent("W");
    expect(chip).toHaveAttribute("title", "Work · bk@work.example");
  });

  test.each(ACCOUNT_SLOTS)("the %s slot draws its own fill and ink", (slot) => {
    render(<AccountMark identity={{ name: "Acct", email: "a@x.example", tag: "A", slot }} />);
    const chip = screen.getByRole("img", { name: "Acct" });
    expect(chip).toHaveClass(`bg-account-${slot}`, `text-account-${slot}-ink`);
    expect(chip).toHaveAttribute("data-account-slot", slot);
  });

  test("every slot has a swatch class too", () => {
    for (const slot of ACCOUNT_SLOTS) expect(SWATCH_CLASS[slot]).toBe(`bg-account-${slot}`);
  });
});
