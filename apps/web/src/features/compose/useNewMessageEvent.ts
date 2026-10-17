import { useEffect } from "react";

import { newMessageIntent, useComposeUi } from "./composeUiStore";

/**
 * The desktop shell's global shortcut dispatches this on the page so a new
 * message opens over the current view without a reload. The shell checks
 * `window.mxrNewMessageReady` first and falls back to navigating to
 * /compose/new while the app has not mounted. A page script can only open an
 * empty composer with it, which `c` does anyway.
 */
export const NEW_MESSAGE_EVENT = "mxr:new-message";

declare global {
  interface Window {
    mxrNewMessageReady?: boolean;
  }
}

/** Mounted in AppShell: ComposeHost itself only mounts once composing starts. */
export function useNewMessageEvent() {
  useEffect(() => {
    const open = () => useComposeUi.getState().openCompose(newMessageIntent(), "overlay");
    window.addEventListener(NEW_MESSAGE_EVENT, open);
    window.mxrNewMessageReady = true;
    return () => {
      window.removeEventListener(NEW_MESSAGE_EVENT, open);
      window.mxrNewMessageReady = false;
    };
  }, []);
}
