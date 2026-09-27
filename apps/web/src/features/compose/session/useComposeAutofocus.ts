/*
 * First focus for a freshly loaded compose session: To for a new message,
 * the body for a reply whose recipients are already filled in.
 */

import { useEffect, useRef, type MutableRefObject, type RefObject } from "react";

import type { ComposeDraftState } from "./composeDraft";

export function useComposeAutofocus(
  draftPath: string | undefined,
  draftRef: MutableRefObject<ComposeDraftState | null>,
  toInputRef: RefObject<HTMLInputElement | null>,
) {
  const hasAutofocusedRef = useRef(false);

  useEffect(() => {
    if (!draftPath || hasAutofocusedRef.current) return;
    hasAutofocusedRef.current = true;
    // Defer past the loading→loaded re-render. Never take focus the user
    // already placed inside this composer; focus left on the mail list or
    // reader (where c / r were pressed) moves here. A new message starts in
    // To; a reply, whose recipients are filled in, starts in the body.
    requestAnimationFrame(() => {
      const active = document.activeElement;
      if (active instanceof HTMLElement && active.closest("[data-compose-surface]")) return;
      if (draftRef.current?.frontmatter.to.trim()) {
        document
          .querySelector<HTMLElement>(
            "[data-compose-surface] .cm-content, [data-compose-surface] .ProseMirror",
          )
          ?.focus();
      } else {
        toInputRef.current?.focus();
      }
    });
  }, [draftPath, draftRef, toInputRef]);
}
