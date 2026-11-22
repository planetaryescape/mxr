import { onWorkspaceChanged } from "./workspaceOwner";
import { clearOfflineMail } from "@/pwa/offlineReads";
import { requestCoordinator } from "./requestCoordinator";
import { onlineManager, type Query, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  apiFetch,
  BridgeRequestError,
  DaemonUnavailableError,
  onDaemonReachability,
} from "@/api/client";
import { daemonEvents } from "@/lib/ws";
import { useConnectionStore } from "@/state/connectionStore";

const PROBE_PATH = "/api/v1/admin/status";
const PROBE_MIN_DELAY_MS = 1_000;
const PROBE_MAX_DELAY_MS = 8_000;

let probing: Promise<void> | null = null;
let probeAttempt = 0;
let plannedStopUntil = 0;
let probeTimer: ReturnType<typeof setTimeout> | undefined;

export function isDaemonDown(): boolean {
  return useConnectionStore.getState().daemonDown;
}

export function useDaemonDown(): boolean {
  return useConnectionStore((state) => state.daemonDown);
}

let lastAnsweredAt = Date.now();

function setDown(daemonDown: boolean): void {
  if (isDaemonDown() === daemonDown) return;
  useConnectionStore.getState().setState({
    daemonDown,
    daemonLastAnsweredAt: daemonDown ? lastAnsweredAt : undefined,
  });
}

export function refuseWhileDaemonDown(what: string, kept?: string): boolean {
  if (!isDaemonDown()) return false;
  toast.warning(`Can't ${what} while mxr is unavailable`, {
    id: "daemon-down-refusal",
    description: kept ?? "Nothing changed. Try again once it's back.",
  });
  return true;
}

function scheduleProbe(): void {
  clearTimeout(probeTimer);
  const base = Math.min(PROBE_MAX_DELAY_MS, PROBE_MIN_DELAY_MS * 2 ** probeAttempt);
  probeAttempt += 1;
  probeTimer = setTimeout(() => void probeDaemon(), base + Math.random() * 0.2 * base);
}

export function probeDaemon(): Promise<void> {
  probing ??= apiFetch(PROBE_PATH)
    .then(
      () => undefined,
      (error: unknown) => {
        const offline = error instanceof BridgeRequestError && error.code === "offline_not_cached";
        if (!(error instanceof DaemonUnavailableError) && !offline) return;
        setDown(true);
        scheduleProbe();
      },
    )
    .finally(() => {
      probing = null;
    });
  return probing;
}

function retainedPrivateQuery(query: Query): boolean {
  return query.meta?.keepThroughGaps === true;
}

export function startDaemonAvailability(queryClient: QueryClient): () => void {
  const offWorkspace = onWorkspaceChanged(() => {
    clearOfflineMail();
    requestCoordinator.cancelAll();
    void queryClient
      .cancelQueries({ predicate: (query) => !retainedPrivateQuery(query) })
      .then(() => {
        queryClient.removeQueries({ predicate: (query) => !retainedPrivateQuery(query) });
        void refetchAfterGap(queryClient);
      });
  });
  const markReachable = () => {
    if (!isDaemonDown()) return;
    if (Date.now() < plannedStopUntil) {
      scheduleProbe();
      return;
    }
    clearTimeout(probeTimer);
    probeAttempt = 0;
    void refetchAfterGap(queryClient, "none");
    setDown(false);
    if (daemonEvents.getStatus().state !== "connected") daemonEvents.reconnectNow();
  };

  onlineManager.setEventListener((setOnline) => {
    setOnline(!isDaemonDown());
    return useConnectionStore.subscribe((state, previous) => {
      if (state.daemonDown !== previous.daemonDown) setOnline(!state.daemonDown);
    });
  });

  const offReachability = onDaemonReachability((reachable) => {
    if (reachable) {
      lastAnsweredAt = Date.now();
      markReachable();
    } else if (!isDaemonDown()) void probeDaemon();
  });

  let lastSocketState = daemonEvents.getStatus().state;
  let eventGapOpen = false;
  const offSocket = daemonEvents.onStatus(({ state }) => {
    const dropped = lastSocketState === "connected" && state !== "connected";
    const recovered = eventGapOpen && state === "connected";
    lastSocketState = state;
    if (dropped) eventGapOpen = true;
    if (recovered) {
      eventGapOpen = false;
      void refetchAfterGap(queryClient);
    }
    if (isDaemonDown() ? state === "connected" : dropped) void probeDaemon();
  });

  const offLifecycle = daemonEvents.subscribe((event) => {
    if (event.type !== "ServerStopping" || typeof event.deadline_at !== "number") return;
    plannedStopUntil = event.deadline_at * 1_000;
    setDown(true);
    scheduleProbe();
    toast.warning("mxr is stopping. Your drafts are kept; mail will refresh when it returns.", {
      id: "daemon-stopping",
    });
  });

  const onFocus = () => {
    if (isDaemonDown()) void probeDaemon();
  };
  window.addEventListener("focus", onFocus);

  return () => {
    offWorkspace();
    offReachability();
    offSocket();
    offLifecycle();
    window.removeEventListener("focus", onFocus);
    clearTimeout(probeTimer);
  };
}

export function refetchAfterGap(
  queryClient: QueryClient,
  refetchType: "active" | "none" = "active",
): Promise<void> {
  return queryClient.invalidateQueries({
    predicate: (query: Query) => query.meta?.keepThroughGaps !== true,
    refetchType,
  });
}

export function resetDaemonAvailabilityForTest(): void {
  clearTimeout(probeTimer);
  probeAttempt = 0;
  probing = null;
  plannedStopUntil = 0;
  useConnectionStore.getState().setState({ daemonDown: false });
}
