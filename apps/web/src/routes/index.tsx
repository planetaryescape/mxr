import { createFileRoute, redirect } from "@tanstack/react-router";

import { useUiPrefs } from "@/state/uiPrefsStore";

export const Route = createFileRoute("/")({
  beforeLoad: () => {
    // Prefs hydrate from storage before the router starts (main.tsx).
    if (useUiPrefs.getState().home === "inbox") {
      throw redirect({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
    }
    throw redirect({ to: "/now" });
  },
});
