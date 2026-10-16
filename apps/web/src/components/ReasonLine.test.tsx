/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { ReasonLine, reasonText } from "./ReasonLine";

describe("ReasonLine", () => {
  test("keeps the daemon's own prefix and adds one when it is missing", () => {
    expect(reasonText("Here because: Samir asked you a question.")).toBe(
      "Here because: Samir asked you a question.",
    );
    expect(reasonText("a new sender, not placed yet")).toBe(
      "Here because: a new sender, not placed yet",
    );
  });

  test("renders in meta type at the secondary tier", () => {
    render(<ReasonLine reason="Here because: a new sender" />);
    expect(screen.getByTestId("reason-line")).toHaveClass("type-meta", "text-text-2");
  });
});
