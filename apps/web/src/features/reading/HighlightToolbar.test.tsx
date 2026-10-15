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
  test("offers Highlight and Highlight with note for article text", () => {
    setup();
    select(screen.getByTestId("inside"));
    expect(screen.getByRole("toolbar")).toBeTruthy();
    fireEvent.click(screen.getByTestId("toolbar-highlight"));
    expect(verbs.highlightSelection).toHaveBeenCalledWith(
      "some text",
      "m:0",
      "issue",
      undefined,
      null,
    );
  });

  test("passes the paragraph the selection starts in, so the highlight is anchored there", () => {
    setup();
    const inside = screen.getByTestId("inside");
    inside.setAttribute("data-paragraph", "4");
    select(inside);
    fireEvent.click(screen.getByTestId("toolbar-highlight"));
    expect(verbs.highlightSelection).toHaveBeenCalledWith(
      "some text",
      "m:0",
      "issue",
      undefined,
      4,
    );
  });

  test("a selection in the highlights list or with nothing selected shows nothing", () => {
    setup();
    select(screen.getByTestId("list"));
    expect(screen.queryByRole("toolbar")).toBeNull();
    window.getSelection()?.removeAllRanges();
    act(() => {
      document.dispatchEvent(new Event("selectionchange"));
    });
    expect(screen.queryByRole("toolbar")).toBeNull();
  });

  test("Highlight with note saves the typed note with the selected text", async () => {
    setup();
    select(screen.getByTestId("inside"));
    fireEvent.click(screen.getByTestId("toolbar-highlight-note"));
    const input = screen.getByTestId("highlight-note-input");
    fireEvent.change(input, { target: { value: "for the talk" } });
    // Typing moved the selection away; the picked text is kept.
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
    // A blank note is sent as "", which clears it.
    expect(verbs.editHighlightNote).toHaveBeenCalledWith("m:0", "own", "");
  });

  test("removing goes by the highlight's id", () => {
    setup();
    select(screen.getByTestId("own"));
    fireEvent.click(screen.getByTestId("toolbar-remove"));
    expect(verbs.removeHighlight).toHaveBeenCalledWith("m:0", "own");
  });

  test("a Marginalia highlight is read-only and points to Marginalia", () => {
    const { onOpenMarginalia } = setup();
    select(screen.getByTestId("theirs"));
    expect(screen.getByText("Made in Marginalia; change it there")).toBeTruthy();
    expect(screen.queryByTestId("toolbar-remove")).toBeNull();
    expect(screen.queryByTestId("toolbar-edit-note")).toBeNull();
    fireEvent.click(screen.getByText("Open in Marginalia"));
    expect(onOpenMarginalia).toHaveBeenCalled();
  });
});
