import type { KeyboardEvent } from "react";
import { isComposingKey } from "@/lib/keys/composition";

export const PENDING_CARD_ATTRIBUTE = "data-pending-card";

export function isTyping(event: KeyboardEvent): boolean {
  if (isComposingKey(event) || event.metaKey || event.ctrlKey || event.altKey) return true;
  const target = event.target;
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName);
}

export function onBareKey(handler: (key: string, event: KeyboardEvent) => boolean) {
  return (event: KeyboardEvent) => {
    if (event.defaultPrevented || event.repeat || isTyping(event)) return;
    if (handler(event.key, event)) {
      event.preventDefault();
      event.stopPropagation();
    }
  };
}
