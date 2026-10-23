import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { CodeMirrorComposeEditor } from "./CodeMirrorComposeEditor";

function setup() {
  const onSend = vi.fn<() => void>();
  const onChange = vi.fn<(value: string) => void>();
  const { container } = render(
    <CodeMirrorComposeEditor
      value="hello"
      onChange={onChange}
      onSave={vi.fn<() => void>()}
      onSend={onSend}
      onDiscard={vi.fn<() => void>()}
    />,
  );
  const content = container.querySelector<HTMLElement>(".cm-content")!;
  return { content, onSend, onChange };
}

describe("CodeMirrorComposeEditor held keys", () => {
  it("a held ⌘↵ sends once", () => {
    const { content, onSend } = setup();
    fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, metaKey: true });
    fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, metaKey: true, repeat: true });
    fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, metaKey: true, repeat: true });
    expect(onSend).toHaveBeenCalledTimes(1);
  });

  it("editor chords' repeats reach the editor exactly as their first press does", () => {
    const { content } = setup();
    const chords = [
      ["z", false],
      ["z", true],
      ["b", false],
      ["i", false],
      ["ArrowLeft", true],
    ] as const;
    // fireEvent returns false when a handler prevented the default.
    const press = (key: string, shiftKey: boolean, repeat: boolean) =>
      fireEvent.keyDown(content, { key, shiftKey, ctrlKey: true, metaKey: true, repeat });
    const first = chords.map(([key, shiftKey]) => press(key, shiftKey, false));
    const held = chords.map(([key, shiftKey]) => press(key, shiftKey, true));
    expect(held).toEqual(first);
  });
});

describe("CodeMirrorComposeEditor model marks", () => {
  it("accepting a longer text leaves a mark on the words it added", () => {
    const { container, rerender } = render(
      <CodeMirrorComposeEditor
        value="Hi"
        onChange={vi.fn<(value: string) => void>()}
        onSave={vi.fn<() => void>()}
        onSend={vi.fn<() => void>()}
        onDiscard={vi.fn<() => void>()}
      />,
    );
    // The accept replaces the text and marks the added words in one render.
    rerender(
      <CodeMirrorComposeEditor
        value="Hi there"
        onChange={vi.fn<(value: string) => void>()}
        onSave={vi.fn<() => void>()}
        onSend={vi.fn<() => void>()}
        onDiscard={vi.fn<() => void>()}
        modelSpans={[{ start: 2, end: 8 }]}
      />,
    );
    const marks = Array.from(container.querySelectorAll(".cm-model-span")).map(
      (node) => node.textContent,
    );
    expect(marks).toEqual([" there"]);
  });
});
