import { QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { useEffect } from "react";

import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useConnectionStatusBootstrap } from "@/hooks/useConnectionStatus";
import { useDaemonEventInvalidation } from "@/hooks/useDaemonEventInvalidation";
import { useProtocolCompatibilityBootstrap } from "@/hooks/useProtocolCompatibility";
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
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={300}>
        <RealtimeBootstrap />
        <RouterProvider router={router} />
        <Toaster />
        {import.meta.env.DEV ? <ReactQueryDevtools buttonPosition="bottom-right" /> : null}
      </TooltipProvider>
    </QueryClientProvider>
  );
}

function RealtimeBootstrap() {
  useConnectionStatusBootstrap();
  useProtocolCompatibilityBootstrap();
  useDaemonEventInvalidation();
  useEffect(() => {
    daemonEvents.start();
    return () => daemonEvents.stop();
  }, []);
  return null;
}
