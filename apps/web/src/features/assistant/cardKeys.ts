/*
 * Keys for a card that waits on the user (a proof, a question). The card
 * owns the keyboard while focus is inside it (`data-keys-owner` keeps the
 * app's own keys out), and only for keys that aren't being typed: a letter
 * typed in the composer or a note field must never apply or send back a
 * change.
 */

import type { KeyboardEvent } from "react";
import { isComposingKey } from "@/lib/keys/composition";

export const PENDING_CARD_ATTRIBUTE = "data-pending-card";

/** True when the key was typed into a field, or is part of a chord or an IME composition. */
export function isTyping(event: KeyboardEvent): boolean {
  if (isComposingKey(event) || event.metaKey || event.ctrlKey || event.altKey) return true;
  const target = event.target;
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName);
}

/** Runs `handler` with the bare key when it isn't typing and isn't a held repeat. */
export function onBareKey(handler: (key: string, event: KeyboardEvent) => boolean) {
  return (event: KeyboardEvent) => {
    if (event.defaultPrevented || event.repeat || isTyping(event)) return;
    if (handler(event.key, event)) {
      event.preventDefault();
      event.stopPropagation();
    }
  };
}
