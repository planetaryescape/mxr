const HOLD_MS = 1500;

function isTextField(target: EventTarget | null): target is HTMLInputElement | HTMLTextAreaElement {
  return (
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLInputElement && /^(text|search|email|url|)$/.test(target.type))
  );
}

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
