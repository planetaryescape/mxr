import { QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { RouterProvider, createRouter, useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";

import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useConnectionStatusBootstrap } from "@/hooks/useConnectionStatus";
import { useDaemonEventInvalidation } from "@/hooks/useDaemonEventInvalidation";
import { useProtocolCompatibilityBootstrap } from "@/hooks/useProtocolCompatibility";
import { startDaemonAvailability } from "@/lib/daemonAvailability";
import { createQueryClient, setActiveQueryClient } from "@/lib/queryClient";
import { daemonEvents } from "@/lib/ws";
import { routeTree } from "@/routeTree.gen";

const queryClient = createQueryClient({
  onUnauthorized: () => {
    const target = "/settings/token?reason=expired";
    if (window.location.pathname + window.location.search !== target) {
      window.location.assign(target);
    }
  },
});
setActiveQueryClient(queryClient);

const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultPreloadStaleTime: 0,
  context: { queryClient },
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

export default function App() {
  const toastPosition = useRouterState({
    router,
    select: (state) => (state.location.pathname === "/focus" ? "top-right" : "bottom-right"),
  });

  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={300}>
        <RealtimeBootstrap />
        <RouterProvider router={router} />
        <Toaster position={toastPosition} />
        {/* Opt-in: the floating button covers the reader's corner in normal dev use. */}
        {import.meta.env.DEV && import.meta.env.VITE_MXR_DEVTOOLS === "1" ? (
          <ReactQueryDevtools buttonPosition="bottom-left" />
        ) : null}
      </TooltipProvider>
    </QueryClientProvider>
  );
}

function RealtimeBootstrap() {
  useConnectionStatusBootstrap();
  useProtocolCompatibilityBootstrap();
  useDaemonEventInvalidation();
  // Before the socket starts, so its first drop is heard.
  useEffect(() => startDaemonAvailability(queryClient), []);
  useEffect(() => {
    daemonEvents.start();
    return () => daemonEvents.stop();
  }, []);
  return null;
}
