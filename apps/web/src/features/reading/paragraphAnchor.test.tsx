import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { selectionParagraph } from "./paragraphAnchor";
import { Paragraphs } from "./ReadingReader";

afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
});

const PARAGRAPHS = [
  { kind: "paragraph", text: "First. Same words here." },
  { kind: "heading", text: "A heading" },
  { kind: "paragraph", text: "Second. Same words here." },
] as never[];

function selectText(node: Node, from: number, to: number) {
  const range = document.createRange();
  range.setStart(node, from);
  range.setEnd(node, to);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  return range;
}

describe("the plain reader's paragraph anchors", () => {
  test("a selection in the second occurrence reports that paragraph's index", () => {
    render(<Paragraphs paragraphs={PARAGRAPHS} marks={[]} />);
    const root = screen.getByTestId("reader-text");
    const second = root.querySelectorAll("[data-paragraph]")[2]!;
    const text = second.firstChild as Text;
    selectText(text, text.data.indexOf("Same"), text.data.length);
    expect(selectionParagraph(root)).toBe(2);
  });
});

describe("drawing anchored marks in the plain reader", () => {
  const marked = (marks: { quote: string; paragraph?: number | null }[]) => {
    render(<Paragraphs paragraphs={PARAGRAPHS} marks={marks} />);
    return [...document.querySelectorAll("mark")].map((mark) =>
      mark.closest("[data-paragraph]")?.getAttribute("data-paragraph"),
    );
  };

  test("an anchored highlight marks only the paragraph it was made in", () => {
    expect(marked([{ quote: "Same words here", paragraph: 2 }])).toEqual(["2"]);
  });
});
