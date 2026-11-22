import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { describe, expect, test } from "vitest";

import { isComposingKey } from "./composition";

function key(init: KeyboardEventInit & { keyCode?: number }): KeyboardEvent {
  const event = new KeyboardEvent("keydown", init);
  // jsdom ignores keyCode in the init dictionary, as browsers do.
  if (init.keyCode !== undefined) Object.defineProperty(event, "keyCode", { value: init.keyCode });
  return event;
}

describe("isComposingKey", () => {
  test("reads a React event's native event", () => {
    const nativeEvent = key({ key: "Enter", keyCode: 229 });
    expect(isComposingKey({ nativeEvent } as unknown as ReactKeyboardEvent)).toBe(true);
  });
});
