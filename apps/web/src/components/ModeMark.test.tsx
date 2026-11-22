/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { ModeMark, ModeMarks } from "./ModeMark";

describe("ModeMark", () => {
  test("is named for its mode, with the daemon's detail as the tooltip", () => {
    render(<ModeMark mode="todo" detail="Send the sync agenda, act by Fri" />);
    const mark = screen.getByRole("img", { name: "To do: Send the sync agenda, act by Fri" });
    expect(mark).toHaveAttribute("title", "Send the sync agenda, act by Fri");
    expect(mark).toHaveClass("text-text-3");
    expect(mark.querySelector("svg")).toHaveClass("size-3.5");
  });

  test("a list of memberships is one mark per mode, and nothing when there are none", () => {
    const { rerender } = render(
      <ModeMarks
        modes={[
          { mode: "todo", also_in: "In To do" },
          { mode: "updates", reason: "Here because: a digest" },
        ]}
      />,
    );
    expect(screen.getAllByTestId("mode-mark").map((mark) => mark.dataset.mode)).toEqual([
      "todo",
      "updates",
    ]);
    rerender(<ModeMarks modes={[]} />);
    expect(screen.queryByTestId("mode-mark")).toBeNull();
  });
});
