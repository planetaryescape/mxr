import { useEffect } from "react";

import type { DaemonEvent, DaemonEventHandler } from "@/api/events";
import { daemonEvents } from "@/lib/ws";

export function useDaemonEvents(handler: DaemonEventHandler): void {
  useEffect(() => daemonEvents.subscribe(handler), [handler]);
}

export type { DaemonEvent };
