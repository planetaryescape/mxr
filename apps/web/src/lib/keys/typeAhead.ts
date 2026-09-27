/*
 * A palette opened from a key takes a frame or two to mount and focus its
 * field, and people type straight on ("⌘K settings"). Keys typed in that
 * gap land on the page and are lost (the dispatcher is suspended while the
 * palette is open). Hold them, then type them into the first text field
 * that takes focus.
 */

const HOLD_MS = 1500;

function isTextField(target: EventTarget | null): target is HTMLInputElement | HTMLTextAreaElement {
  return (
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLInputElement && /^(text|search|email|url|)$/.test(target.type))
  );
}

/** Insert text the way typing would, so React and cmdk see an input event. */
function typeInto(field: HTMLInputElement | HTMLTextAreaElement, text: string): void {
  const proto = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  const setValue = Object.getOwnPropertyDescriptor(proto.prototype, "value")?.set;
  setValue?.call(field, field.value + text);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

let release: (() => void) | null = null;

export function holdTypeAhead(target: Window = window): void {
  release?.();
  let held = "";
  // "⌘K arch⏎" typed in one go: Enter must pick from the filtered list,
  // not reach the page (or an unfiltered palette) ahead of the letters.
  let enterHeld = false;
  const onKeyDown = (event: KeyboardEvent) => {
    if (isTextField(event.target)) {
      stop();
      return;
    }
    const plain = !event.metaKey && !event.ctrlKey && !event.altKey;
    const printable = event.key.length === 1 && plain;
    const enter = event.key === "Enter" && plain && held.length > 0;
    if (!printable && !enter) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (enter) enterHeld = true;
    else if (!enterHeld) held += event.key;
  };
  const onFocusIn = (event: FocusEvent) => {
    if (!isTextField(event.target)) return;
    const field = event.target;
    const text = held;
    const enter = enterHeld;
    stop();
    if (text) typeInto(field, text);
    // Let the list filter on the typed text before choosing from it.
    if (enter) {
      setTimeout(() =>
        field.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
        ),
      );
    }
  };
  const timer = setTimeout(() => stop(), HOLD_MS);
  const stop = () => {
    clearTimeout(timer);
    target.removeEventListener("keydown", onKeyDown, true);
    target.removeEventListener("focusin", onFocusIn, true);
    if (release === stop) release = null;
  };
  release = stop;
  target.addEventListener("keydown", onKeyDown, true);
  target.addEventListener("focusin", onFocusIn, true);
}
