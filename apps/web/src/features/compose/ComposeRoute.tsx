/*
 * `/compose/new?...` and `/compose/$draftId` are deep links, not a second
 * composer. They open the one compose surface (ComposeHost, mounted in
 * AppShell) with the matching intent, then step off the compose URL: back
 * to where the user came from inside the app, or to the inbox for a link
 * opened from outside.
 */

import { useCanGoBack, useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { useEffect, useRef } from "react";

import { intentFromComposeLocation, useComposeUi } from "./composeUiStore";

export function ComposeRoute() {
  const location = useRouterState({ select: (state) => state.location });
  const navigate = useNavigate();
  const router = useRouter();
  const canGoBack = useCanGoBack();
  const openCompose = useComposeUi((state) => state.openCompose);
  // Opening is a one-shot side effect of arriving on the URL; the ref also
  // absorbs StrictMode's dev double-invoke.
  const handledRef = useRef(false);

  useEffect(() => {
    if (handledRef.current) return;
    handledRef.current = true;
    openCompose(
      intentFromComposeLocation(location.pathname, location.search as Record<string, unknown>),
      "overlay",
    );
    if (canGoBack) {
      router.history.back();
    } else {
      void navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" }, replace: true });
    }
  }, [canGoBack, location.pathname, location.search, navigate, openCompose, router]);

  return null;
}
