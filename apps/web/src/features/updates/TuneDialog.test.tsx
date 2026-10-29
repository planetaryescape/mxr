import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { UpdateLine } from "./api";
import { TuneDialog } from "./UpdatesDialogs";

const line = { source_name: "GitHub acme/api", setting: "digest" } as unknown as UpdateLine;

describe("TuneDialog", () => {
  it("moves between its choices with j, k and the arrows, as every picker does", () => {
    render(<TuneDialog line={line} onOpenChange={vi.fn()} onChoose={vi.fn()} />);
    const choices = screen.getAllByRole("button").filter((button) => button.closest(".grid"));
    choices[0]!.focus();

    fireEvent.keyDown(choices[0]!, { key: "j" });
    expect(choices[1]).toHaveFocus();
    fireEvent.keyDown(choices[1]!, { key: "ArrowDown" });
    expect(choices[2]).toHaveFocus();
    fireEvent.keyDown(choices[2]!, { key: "k" });
    expect(choices[1]).toHaveFocus();
  });
});
