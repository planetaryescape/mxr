import { useEffect } from "react";

import { newMessageIntent, useComposeUi } from "./composeUiStore";

export const NEW_MESSAGE_EVENT = "mxr:new-message";

declare global {
  interface Window {
    mxrNewMessageReady?: boolean;
  }
}

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
