/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { SectionHeading } from "./SectionHeading";

describe("SectionHeading", () => {
  test("a count of 0 is shown, an absent count is not", () => {
    const { rerender } = render(<SectionHeading count={0}>Needs a look</SectionHeading>);
    expect(
      screen.getByRole("heading", { name: /Needs a look/ }).querySelector("span"),
    ).toHaveTextContent("0");
    rerender(<SectionHeading>Routine</SectionHeading>);
    expect(screen.getByRole("heading", { name: "Routine" }).querySelector("span")).toBeNull();
  });

});
