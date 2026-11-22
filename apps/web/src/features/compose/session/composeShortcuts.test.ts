import type { KeyboardEvent } from "react";
import { describe, expect, it, vi } from "vitest";

import {
  composeCommandOf,
  handleComposeShortcut,
  type ComposeShortcutHandlers,
} from "./composeShortcuts";

function handlers(): ComposeShortcutHandlers {
  return {
    busy: false,
    canServerSave: true,
    revealCc: vi.fn<() => void>(),
    revealBcc: vi.fn<() => void>(),
    handleAttachShortcut: vi.fn<() => void>(),
    requestSendLater: vi.fn<() => void>(),
    openSignaturePicker: vi.fn<() => void>(),
    openDraftHelp: vi.fn<() => void>(),
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

  it("⌘⇧⌫ is a discard chord everywhere, so its repeats are swallowed too", () => {
    expect(
      composeCommandOf({ key: "Backspace", shiftKey: true, metaKey: true, ctrlKey: false }),
    ).toBe("requestDiscard");
    const bag = handlers();
    const held = keyEvent({ key: "Backspace", shiftKey: true, repeat: true });
    handleComposeShortcut(held.event, bag);
    expect(held.preventDefault).toHaveBeenCalled();
    expect(bag.requestDiscard).not.toHaveBeenCalled();
  });
});
