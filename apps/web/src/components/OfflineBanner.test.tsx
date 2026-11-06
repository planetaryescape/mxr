/* @vitest-environment jsdom */

import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useConnectionStore } from "@/state/connectionStore";

import { OfflineBanner } from "./OfflineBanner";

const bridge = vi.hoisted(() => ({ url: "http://localhost" }));
vi.mock("@/lib/tokenStorage", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/tokenStorage")>()),
  getBridgeBaseUrl: () => bridge.url,
}));

function banner(kind: string): Element | null {
  return document.querySelector(`[data-offline-banner="${kind}"]`);
}

beforeEach(() => {
  bridge.url = "http://localhost";
  vi.useFakeTimers();
  useConnectionStore.setState({
    state: "connected",
    daemonDown: false,
    protocolMismatch: undefined,
  });
});

test("a hosted app explains offline reading without asking the phone to run a daemon", () => {
  bridge.url = "https://mxr.example.test";
  useConnectionStore.setState({ daemonDown: true });
  render(<OfflineBanner />);
  expect(banner("daemon")).toHaveTextContent("Saved mail stays readable");
  expect(banner("daemon")).toHaveTextContent("reconnects automatically");
  expect(document.body).not.toHaveTextContent("Copy start command");
});

afterEach(() => {
  vi.useRealTimers();
});

test("an event stream down 30s with the daemon answering pauses live updates, and says only that", () => {
  render(<OfflineBanner />);
  act(() => useConnectionStore.getState().setState({ state: "reconnecting" }));

  // A short drop is ordinary reconnecting: nothing to say yet.
  act(() => vi.advanceTimersByTime(29_000));
  expect(document.querySelector("[data-offline-banner]")).toBeNull();

  act(() => vi.advanceTimersByTime(2_000));
  expect(banner("stream")).toHaveTextContent("Live updates are paused while mxr reconnects");
  // Requests still reach the daemon, so it isn't reported as stopped.
  expect(banner("daemon")).toBeNull();

  act(() => useConnectionStore.getState().setState({ state: "connected" }));
  act(() => vi.advanceTimersByTime(200));
  expect(document.querySelector("[data-offline-banner]")).toBeNull();
});
