/* @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { returnFocusFromFrame } from "./messageFrameFocus";

function mountFrame(html: string) {
  const pane = document.createElement("div");
  pane.tabIndex = -1;
  const frame = document.createElement("iframe");
  pane.append(frame);
  document.body.append(pane);
  const inner = frame.contentDocument!;
  inner.body.innerHTML = html;
  return { pane, frame, inner };
}

/** What a click in the message does as far as the page can see. */
function clickIntoFrame(frame: HTMLIFrameElement) {
  frame.focus();
  window.dispatchEvent(new FocusEvent("blur"));
}

function copyEvent() {
  const data = new Map<string, string>();
  const event = new Event("copy", { cancelable: true }) as ClipboardEvent;
  Object.defineProperty(event, "clipboardData", {
    value: { setData: (type: string, value: string) => data.set(type, value) },
  });
  return { event, data };
}

describe("returnFocusFromFrame", () => {
  let cleanup: (() => void) | undefined;
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    cleanup?.();
    cleanup = undefined;
    document.getSelection()?.removeAllRanges();
    document.body.innerHTML = "";
    vi.useRealTimers();
  });

  it("hands focus back to the reader pane after a click lands in the message", () => {
    const { pane, frame } = mountFrame("<p>hello</p>");
    const onReturn = vi.fn<() => void>();
    cleanup = returnFocusFromFrame(frame, onReturn);

    clickIntoFrame(frame);
    // Not during the click itself: WebKit would put focus straight back.
    expect(document.activeElement).toBe(frame);
    vi.runAllTimers();

    expect(document.activeElement).toBe(pane);
    expect(onReturn).toHaveBeenCalledTimes(1);
  });

  it("leaves focus on a link the keyboard reached inside the message", () => {
    const { frame, inner } = mountFrame('<a href="https://example.com">a link</a>');
    const onReturn = vi.fn<() => void>();
    cleanup = returnFocusFromFrame(frame, onReturn);

    clickIntoFrame(frame);
    inner.querySelector("a")!.focus();
    vi.runAllTimers();

    expect(document.activeElement).toBe(frame);
    expect(onReturn).not.toHaveBeenCalled();
  });

  it("ignores the window losing focus to another app", () => {
    const { frame } = mountFrame("<p>hello</p>");
    const onReturn = vi.fn<() => void>();
    cleanup = returnFocusFromFrame(frame, onReturn);

    window.dispatchEvent(new FocusEvent("blur"));
    vi.runAllTimers();

    expect(onReturn).not.toHaveBeenCalled();
  });

  it("copies the text selected in the message once the page holds focus", () => {
    const { frame, inner } = mountFrame("<p>hello <b>there</b> world</p>");
    cleanup = returnFocusFromFrame(frame);
    clickIntoFrame(frame);
    vi.runAllTimers();
    const range = inner.createRange();
    range.selectNodeContents(inner.querySelector("b")!);
    inner.getSelection()!.addRange(range);

    const before = new Event("beforecopy", { cancelable: true });
    document.dispatchEvent(before);
    const { event, data } = copyEvent();
    document.dispatchEvent(event);

    // Cancelling beforecopy is what enables Copy in Safari.
    expect(before.defaultPrevented).toBe(true);
    expect(event.defaultPrevented).toBe(true);
    expect(data.get("text/plain")).toBe("there");
    expect(data.get("text/html")).toBe("there");
  });

  it("leaves copy alone when the page has its own selection or the message has none", () => {
    const { frame, inner } = mountFrame("<p>hello</p>");
    const outside = document.createElement("p");
    outside.textContent = "page text";
    document.body.append(outside);
    cleanup = returnFocusFromFrame(frame);
    clickIntoFrame(frame);
    vi.runAllTimers();

    const empty = copyEvent();
    document.dispatchEvent(empty.event);
    expect(empty.event.defaultPrevented).toBe(false);

    const range = inner.createRange();
    range.selectNodeContents(inner.querySelector("p")!);
    inner.getSelection()!.addRange(range);
    const pageRange = document.createRange();
    pageRange.selectNodeContents(outside);
    document.getSelection()!.removeAllRanges();
    document.getSelection()!.addRange(pageRange);
    const both = copyEvent();
    document.dispatchEvent(both.event);
    expect(both.event.defaultPrevented).toBe(false);
    expect(both.data.size).toBe(0);
  });
});
