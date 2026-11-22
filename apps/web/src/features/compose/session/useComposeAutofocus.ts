import { useEffect, useRef, type MutableRefObject, type RefObject } from "react";

import type { ComposeDraftState } from "./composeDraft";

const BODY_SELECTOR = "[data-compose-surface] .cm-content, [data-compose-surface] .ProseMirror";

export const EDITOR_MOUNT_WAIT_MS = 5000;

export function useComposeAutofocus(
  draftPath: string | undefined,
  draftRef: MutableRefObject<ComposeDraftState | null>,
  toInputRef: RefObject<HTMLInputElement | null>,
) {
  const hasAutofocusedRef = useRef(false);

  useEffect(() => {
    if (!draftPath || hasAutofocusedRef.current) return;
    hasAutofocusedRef.current = true;
    let observer: MutationObserver | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const stopWaiting = () => {
      observer?.disconnect();
      observer = null;
      if (timer !== null) clearTimeout(timer);
      timer = null;
    };
    const focusIsElsewhere = () => {
      const active = document.activeElement;
      if (!(active instanceof HTMLElement)) return false;
      if (active.closest("[data-compose-surface]")) return true;
      return active.isContentEditable || active.closest("input, textarea, select") !== null;
    };
    const focusBody = () => {
      if (focusIsElsewhere()) return true;
      const body = document.querySelector<HTMLElement>(BODY_SELECTOR);
      if (!body) return false;
      body.focus();
      return true;
    };
    const frame = requestAnimationFrame(() => {
      if (!draftRef.current?.frontmatter.to.trim()) {
        if (!focusIsElsewhere()) toInputRef.current?.focus();
        return;
      }
      if (focusBody()) return;
      observer = new MutationObserver(() => {
        if (focusBody()) stopWaiting();
      });
      observer.observe(document.body, { childList: true, subtree: true });
      timer = setTimeout(stopWaiting, EDITOR_MOUNT_WAIT_MS);
    });
    return () => {
      cancelAnimationFrame(frame);
      stopWaiting();
    };
  }, [draftPath, draftRef, toInputRef]);
}
