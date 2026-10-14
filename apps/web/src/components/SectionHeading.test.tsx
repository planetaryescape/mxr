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

  test("keeps the heading level the page outline needs", () => {
    render(<SectionHeading as="h4">Sources</SectionHeading>);
    expect(screen.getByRole("heading", { level: 4, name: "Sources" })).toBeInTheDocument();
  });

  test("the right slot sits beside the label and keeps its own content", () => {
    render(<SectionHeading actions={<a href="/x">more</a>}>People</SectionHeading>);
    expect(screen.getByRole("heading", { name: "People" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "more" })).toBeInTheDocument();
  });
});
