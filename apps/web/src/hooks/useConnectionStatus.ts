import { useEffect } from "react";

import { useConnectionStore } from "@/state/connectionStore";
import { daemonEvents } from "@/lib/ws";

export function useConnectionStatusBootstrap(): void {
  const setState = useConnectionStore((s) => s.setState);
  useEffect(() => daemonEvents.onStatus(setState), [setState]);
}

export function useConnectionState() {
  return useConnectionStore((s) => s.state);
}
