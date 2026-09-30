/*
 * Whether mxr's daemon can answer right now, as one app-wide state.
 *
 * Any request that gets no answer, or the event socket dropping, makes this
 * probe the daemon once. Only a failed probe marks it down, so one lost
 * request doesn't pause the app. While down:
 *
 * - React Query's online flag is off, so queries keep their cached data and
 *   pause instead of failing (lists and open threads stay readable).
 * - Mutations don't queue: callers check `isDaemonDown()` and say plainly
 *   what can't run yet.
 * - The probe repeats on a bounded backoff, and again whenever the event
 *   socket reconnects or the window regains focus.
 *
 * When a probe (or any request) gets an answer, queries resume, everything
 * shown is refetched (events sent while down were lost), and the event
 * socket reconnects without waiting out its own backoff.
 */

import { onlineManager, type Query, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { apiFetch, DaemonUnavailableError, onDaemonReachability } from "@/api/client";
import { daemonEvents } from "@/lib/ws";
import { useConnectionStore } from "@/state/connectionStore";

/**
 * Goes all the way to the daemon, not just the bridge (which can be up
 * without it). Sent only to check a suspected outage and while one lasts.
 */
const PROBE_PATH = "/api/v1/admin/status";
const PROBE_MIN_DELAY_MS = 1_000;
const PROBE_MAX_DELAY_MS = 8_000;

let probing: Promise<void> | null = null;
let probeAttempt = 0;
let probeTimer: ReturnType<typeof setTimeout> | undefined;

export function isDaemonDown(): boolean {
  return useConnectionStore.getState().daemonDown;
}

/** True while the daemon is unreachable. Changes only when that flips. */
export function useDaemonDown(): boolean {
  return useConnectionStore((state) => state.daemonDown);
}

function setDown(daemonDown: boolean): void {
  if (isDaemonDown() !== daemonDown) useConnectionStore.getState().setState({ daemonDown });
}

/**
 * Refuse a change while the daemon is down, and say so, rather than let it
 * fail late or wait in a hidden queue. `what` completes "Can't … while";
 * `kept` says what is safe when something is (a draft's text).
 */
export function refuseWhileDaemonDown(what: string, kept?: string): boolean {
  if (!isDaemonDown()) return false;
  // One id: a held or repeated key replaces the toast instead of stacking.
  toast.warning(`Can't ${what} while mxr's daemon is stopped`, {
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

/** Ask the daemon directly; mark it down only if nothing answers. */
export function probeDaemon(): Promise<void> {
  probing ??= apiFetch(PROBE_PATH)
    .then(
      () => undefined,
      (error: unknown) => {
        // Any other failure is still an answer from the daemon.
        if (!(error instanceof DaemonUnavailableError)) return;
        setDown(true);
        scheduleProbe();
      },
    )
    .finally(() => {
      probing = null;
    });
  return probing;
}

/**
 * Keep lists and open threads on screen through a daemon outage and bring
 * everything back when it returns. Call once, with the app's query client.
 */
export function startDaemonAvailability(queryClient: QueryClient): () => void {
  const markReachable = () => {
    if (!isDaemonDown()) return;
    clearTimeout(probeTimer);
    probeAttempt = 0;
    // Mark what is shown stale first, so going online refetches it once:
    // events sent while down were lost.
    void refetchAfterGap(queryClient, "none");
    setDown(false);
    if (daemonEvents.getStatus().state !== "connected") daemonEvents.reconnectNow();
  };

  // Replaces the browser's online/offline events: for mail on this machine
  // (or on a host the bridge serves), the daemon is what "online" means.
  onlineManager.setEventListener((setOnline) => {
    setOnline(!isDaemonDown());
    return useConnectionStore.subscribe((state, previous) => {
      if (state.daemonDown !== previous.daemonDown) setOnline(!state.daemonDown);
    });
  });

  const offReachability = onDaemonReachability((reachable) => {
    if (reachable) markReachable();
    else if (!isDaemonDown()) void probeDaemon();
  });

  // While down the probe timer sets the pace; a socket that opens is only
  // a hint to check now.
  let lastSocketState = daemonEvents.getStatus().state;
  const offSocket = daemonEvents.onStatus(({ state }) => {
    const dropped = lastSocketState === "connected" && state !== "connected";
    lastSocketState = state;
    if (isDaemonDown() ? state === "connected" : dropped) void probeDaemon();
  });

  const onFocus = () => {
    if (isDaemonDown()) void probeDaemon();
  };
  window.addEventListener("focus", onFocus);

  return () => {
    offReachability();
    offSocket();
    window.removeEventListener("focus", onFocus);
    clearTimeout(probeTimer);
  };
}

/**
 * Refresh every query after a gap in the event stream (an outage, lagged
 * events), except those that opt out with `meta.keepThroughGaps`: an open
 * compose session, whose refetch would put the saved file back over text
 * typed since.
 */
export function refetchAfterGap(
  queryClient: QueryClient,
  refetchType: "active" | "none" = "active",
): Promise<void> {
  return queryClient.invalidateQueries({
    predicate: (query: Query) => query.meta?.keepThroughGaps !== true,
    refetchType,
  });
}

/** Tests only: start from "up" with nothing scheduled. */
export function resetDaemonAvailabilityForTest(): void {
  clearTimeout(probeTimer);
  probeAttempt = 0;
  probing = null;
  useConnectionStore.getState().setState({ daemonDown: false });
}
