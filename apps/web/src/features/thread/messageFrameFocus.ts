/**
 * The page side of keyboard focus for a sandboxed message frame.
 *
 * WebKit (Safari, the macOS desktop app's WKWebView) runs no event listeners
 * inside a frame sandboxed without `allow-scripts`, not even ones the page
 * attached. Once a click puts focus in the message, keys go to the frame and
 * never reach the app's dispatcher. So focus goes back to the page right
 * after a click lands in the frame, and ⌘C, which the page now receives,
 * copies the text selected in the message.
 */

/** The frame whose selection ⌘C copies while the page has none of its own. */
let selectionFrame: HTMLIFrameElement | null = null;

export function returnFocusFromFrame(frame: HTMLIFrameElement, onReturn?: () => void): () => void {
  const doc = frame.ownerDocument;
  const view = doc.defaultView;
  if (!view) return () => {};
  let timer: number | undefined;

  const onBlur = () => {
    view.clearTimeout(timer);
    // Wait out the mousedown that moved focus: WebKit puts focus back in the
    // frame when the page takes it during that event.
    timer = view.setTimeout(() => {
      if (doc.activeElement !== frame) return;
      selectionFrame = frame;
      // Tab reached a link in the message: leave it there so the link can be
      // followed from the keyboard.
      const inner = frame.contentDocument?.activeElement;
      if (inner && inner !== frame.contentDocument?.body) return;
      onReturn?.();
      const pane = frame.parentElement?.closest<HTMLElement>("[tabindex]");
      if (pane) pane.focus({ preventScroll: true });
      else frame.blur();
    }, 0);
  };

  const frameSelection = (): Selection | null => {
    if (selectionFrame !== frame) return null;
    if (doc.getSelection()?.isCollapsed === false) return null;
    const selection = frame.contentDocument?.getSelection();
    return selection && !selection.isCollapsed ? selection : null;
  };

  // Safari enables Copy for a page with no selection only when beforecopy
  // is cancelled.
  const onBeforeCopy = (event: Event) => {
    if (frameSelection()) event.preventDefault();
  };

  const onCopy = (event: ClipboardEvent) => {
    const selection = frameSelection();
    if (!selection || !event.clipboardData) return;
    const fragment = doc.createElement("div");
    for (let i = 0; i < selection.rangeCount; i++) {
      fragment.append(selection.getRangeAt(i).cloneContents());
    }
    event.clipboardData.setData("text/plain", selection.toString());
    event.clipboardData.setData("text/html", fragment.innerHTML);
    event.preventDefault();
  };

  view.addEventListener("blur", onBlur);
  doc.addEventListener("beforecopy", onBeforeCopy);
  doc.addEventListener("copy", onCopy);
  return () => {
    view.clearTimeout(timer);
    view.removeEventListener("blur", onBlur);
    doc.removeEventListener("beforecopy", onBeforeCopy);
    doc.removeEventListener("copy", onCopy);
    if (selectionFrame === frame) selectionFrame = null;
  };
}
