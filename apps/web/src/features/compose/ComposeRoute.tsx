import { useCanGoBack, useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { useEffect, useRef } from "react";

import { intentFromComposeLocation, useComposeUi } from "./composeUiStore";

export function ComposeRoute() {
  const location = useRouterState({ select: (state) => state.location });
  const navigate = useNavigate();
  const router = useRouter();
  const canGoBack = useCanGoBack();
  const openCompose = useComposeUi((state) => state.openCompose);
  const handledRef = useRef(false);

  useEffect(() => {
    if (handledRef.current) return;
    handledRef.current = true;
    const intent = intentFromComposeLocation(location.pathname, location.search);
    openCompose(intent, intent.kind === "new" ? "overlay" : "inline");
    if (canGoBack) {
      router.history.back();
    } else {
      void navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" }, replace: true });
    }
  }, [canGoBack, location.pathname, location.search, navigate, openCompose, router]);

  return null;
}
