import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { paragraphOfNode, paragraphOfRangeStart, selectionParagraph } from "./paragraphAnchor";
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

  test("a range that starts at an element boundary uses the child it points to", () => {
    render(<Paragraphs paragraphs={PARAGRAPHS} marks={[]} />);
    const root = screen.getByTestId("reader-text");
    const range = document.createRange();
    range.setStart(root, 1);
    range.setEnd(root, 3);
    expect(paragraphOfRangeStart(range)).toBe(1);
  });

  test("text outside the reader, or outside any paragraph, has no anchor", () => {
    render(
      <>
        <p data-testid="elsewhere">outside</p>
        <Paragraphs paragraphs={PARAGRAPHS} marks={[]} />
      </>,
    );
    const outside = screen.getByTestId("elsewhere");
    selectText(outside.firstChild!, 0, 3);
    expect(paragraphOfNode(outside)).toBeNull();
    expect(selectionParagraph(screen.getByTestId("reader-text"))).toBeNull();
  });

  test("the HTML reader's one-based data-para labels become zero-based", () => {
    const host = document.createElement("div");
    host.innerHTML = `<p data-para="1">a</p><p data-para="2"><b>b</b></p>`;
    document.body.append(host);
    expect(paragraphOfNode(host.querySelector("b"))).toBe(1);
    host.remove();
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

  test("a highlight with no anchor marks every occurrence, as before", () => {
    expect(marked([{ quote: "Same words here" }])).toEqual(["0", "2"]);
  });
});
