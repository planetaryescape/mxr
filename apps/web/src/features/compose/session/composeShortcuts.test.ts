import type { KeyboardEvent } from "react";
import { describe, expect, it, vi } from "vitest";

import { handleComposeShortcut, type ComposeShortcutHandlers } from "./composeShortcuts";

function handlers(): ComposeShortcutHandlers {
  return {
    busy: false,
    canServerSave: true,
    revealCc: vi.fn<() => void>(),
    revealBcc: vi.fn<() => void>(),
    handleAttachShortcut: vi.fn<() => void>(),
    requestSendLater: vi.fn<() => void>(),
    openSignaturePicker: vi.fn<() => void>(),
    handleRefreshClick: vi.fn<() => Promise<void>>(async () => {}),
    handleServerSaveClick: vi.fn<() => Promise<void>>(async () => {}),
    openSnippetPicker: vi.fn<() => void>(),
    handleSaveClick: vi.fn<() => Promise<void>>(async () => {}),
    requestSendAndArchive: vi.fn<() => void>(),
    requestSend: vi.fn<() => void>(),
    requestDiscard: vi.fn<() => void>(),
  };
}

function keyEvent(init: Partial<KeyboardEvent<HTMLDivElement>>) {
  const preventDefault = vi.fn<() => void>();
  const event = {
    defaultPrevented: false,
    metaKey: true,
    ctrlKey: false,
    shiftKey: false,
    repeat: false,
    key: "Enter",
    preventDefault,
    stopPropagation: vi.fn<() => void>(),
    ...init,
  } as unknown as KeyboardEvent<HTMLDivElement>;
  return { event, preventDefault };
}

describe("compose shortcuts", () => {
  it("a held ⌘↵ sends once and claims its repeats", () => {
    const bag = handlers();
    handleComposeShortcut(keyEvent({}).event, bag);
    const repeat = keyEvent({ repeat: true });
    handleComposeShortcut(repeat.event, bag);
    handleComposeShortcut(keyEvent({ repeat: true }).event, bag);
    expect(bag.requestSend).toHaveBeenCalledTimes(1);
    expect(repeat.preventDefault).toHaveBeenCalled();
  });

  it("no compose chord runs on a repeat", () => {
    const bag = handlers();
    for (const [key, shiftKey] of [
      ["Enter", true],
      ["Backspace", false],
      ["s", false],
      ["s", true],
      ["l", true],
      [";", false],
    ] as const) {
      handleComposeShortcut(keyEvent({ key, shiftKey, repeat: true }).event, bag);
    }
    const calls = Object.values(bag).filter((value) => typeof value === "function");
    expect(calls.every((fn) => (fn as ReturnType<typeof vi.fn>).mock.calls.length === 0)).toBe(
      true,
    );
  });
});
