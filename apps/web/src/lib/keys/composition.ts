/*
 * IME composition. While someone types Japanese, Chinese or Korean, Enter
 * picks the candidate and letters build the word; neither is a shortcut or
 * a submit. `isComposing` alone misses the keystrokes at the edges of a
 * composition (WebKit, which the desktop shell uses, reports them as
 * `isComposing: false` with `keyCode` 229), so both are checked.
 */

import type { KeyboardEvent as ReactKeyboardEvent } from "react";

const IME_KEY_CODE = 229;

/** True when the key belongs to an IME composition, not to the app. */
export function isComposingKey(event: KeyboardEvent | ReactKeyboardEvent): boolean {
  const native = "nativeEvent" in event ? event.nativeEvent : event;
  return native.isComposing || native.keyCode === IME_KEY_CODE;
}
