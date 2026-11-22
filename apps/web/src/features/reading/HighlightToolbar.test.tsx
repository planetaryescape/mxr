import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { ReadingHighlight } from "./api";
import { HighlightToolbar } from "./HighlightToolbar";

const verbs = vi.hoisted(() => ({
  highlightSelection: vi.fn<(...args: unknown[]) => Promise<boolean>>().mockResolvedValue(true),
  editHighlightNote: vi.fn<(...args: unknown[]) => Promise<boolean>>().mockResolvedValue(true),
  removeHighlight: vi.fn<(...args: unknown[]) => Promise<boolean>>().mockResolvedValue(true),
}));
vi.mock("./readingVerbs", () => verbs);

const own = { id: "own", quote: "a delete", note: "first", origin: "mxr" } as ReadingHighlight;
const theirs = {
  id: "theirs",
  quote: "an absence",
  note: null,
  origin: "marginalia",
} as ReadingHighlight;

function select(node: Node) {
  const range = document.createRange();
  range.selectNodeContents(node);
  // jsdom has no layout.
  range.getBoundingClientRect = () => new DOMRect(100, 200, 80, 20);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  act(() => {
    document.dispatchEvent(new Event("selectionchange"));
  });
}

function setup(highlights: ReadingHighlight[] = [own, theirs]) {
  const onOpenMarginalia = vi.fn<() => void>();
  function Page() {
    const ref = useRef<HTMLDivElement>(null);
    return (
      <>
        <div ref={ref}>
          <p data-testid="inside">some text</p>
          <p data-testid="own">a delete</p>
          <p data-testid="theirs">an absence</p>
        </div>
        <ul>
          <li data-testid="list">your highlights list</li>
        </ul>
        <HighlightToolbar
          containerRef={ref}
          itemKey="m:0"
          view="issue"
          highlights={highlights}
          onOpenMarginalia={onOpenMarginalia}
        />
      </>
    );
  }
  render(<Page />);
  return { onOpenMarginalia };
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
});

describe("HighlightToolbar", () => {
  test("Highlight with note saves the typed note with the selected text", async () => {
    setup();
    select(screen.getByTestId("inside"));
    fireEvent.click(screen.getByTestId("toolbar-highlight-note"));
    const input = screen.getByTestId("highlight-note-input");
    fireEvent.change(input, { target: { value: "for the talk" } });
    window.getSelection()?.removeAllRanges();
    await act(async () => {
      fireEvent.keyDown(input, { key: "Enter" });
    });
    expect(verbs.highlightSelection).toHaveBeenCalledWith(
      "some text",
      "m:0",
      "issue",
      "for the talk",
      null,
    );
  });

  test("selecting one of your highlights edits its note or removes it", async () => {
    setup();
    select(screen.getByTestId("own"));
    expect(screen.queryByTestId("toolbar-highlight")).toBeNull();
    fireEvent.click(screen.getByTestId("toolbar-edit-note"));
    const input = screen.getByTestId<HTMLTextAreaElement>("highlight-note-input");
    expect(input.value).toBe("first");
    fireEvent.change(input, { target: { value: "" } });
    await act(async () => {
      fireEvent.click(screen.getByTestId("highlight-note-save"));
    });
    expect(verbs.editHighlightNote).toHaveBeenCalledWith("m:0", "own", "");
  });
});
