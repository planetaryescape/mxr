import { fireEvent, render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { TiptapComposeEditor } from "./TiptapComposeEditor";

describe("TiptapComposeEditor held keys", () => {
  it("held ⌘↵ and ⌘⇧⌫ run once; the editor's own chords repeat", async () => {
    const onSend = vi.fn<() => void>();
    const onDiscard = vi.fn<() => void>();
    const { container } = render(
      <TiptapComposeEditor
        value="hello"
        onChange={vi.fn<(value: string) => void>()}
        onSave={vi.fn<() => void>()}
        onSend={onSend}
        onDiscard={onDiscard}
      />,
    );
    await waitFor(() => expect(container.querySelector(".ProseMirror")).not.toBeNull());
    const editor = container.querySelector<HTMLElement>(".ProseMirror")!;
    const press = (key: string, shiftKey: boolean, repeat: boolean) =>
      fireEvent.keyDown(editor, { key, shiftKey, ctrlKey: true, metaKey: true, repeat });

    press("Enter", false, false);
    press("Enter", false, true);
    press("Backspace", true, false);
    press("Backspace", true, true);
    expect(onSend).toHaveBeenCalledTimes(1);
    expect(onDiscard).toHaveBeenCalledTimes(1);

    const native = [
      ["z", false],
      ["z", true],
      ["b", false],
      ["i", false],
    ] as const;
    const first = native.map(([key, shiftKey]) => press(key, shiftKey, false));
    const held = native.map(([key, shiftKey]) => press(key, shiftKey, true));
    expect(held).toEqual(first);
  });
});
