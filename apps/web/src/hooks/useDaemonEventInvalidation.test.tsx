/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { DaemonEvent, DaemonEventHandler } from "@/api/events";
import { useConnectionStore } from "@/state/connectionStore";

import { useDaemonEventInvalidation } from "./useDaemonEventInvalidation";

// Capture the handler the hook registers so tests can drive events
// through it directly, the way the WebSocket stream would.
const events = vi.hoisted(() => ({ handler: undefined as DaemonEventHandler | undefined }));
vi.mock("@/hooks/useDaemonEvents", () => ({
  useDaemonEvents: (handler: DaemonEventHandler) => {
    events.handler = handler;
  },
}));

const toastMock = vi.hoisted(() => ({ error: vi.fn<(message: string) => void>() }));
vi.mock("sonner", () => ({ toast: toastMock }));

function Harness(): null {
  useDaemonEventInvalidation();
  return null;
}

function setup() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const invalidate = vi.spyOn(queryClient, "invalidateQueries").mockResolvedValue();
  const wrapper = (children: ReactNode) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  render(wrapper(<Harness />));
  const emit = (event: DaemonEvent) => events.handler?.(event);
  // Mail events invalidate by predicate (the shared mail query families),
  // so report which of a representative set of cached keys each call hits.
  const mailKeys = [["mailbox"], ["thread"], ["search"], ["reply-queue"], ["shell"], ["settings"]];
  const invalidatedKeys = () =>
    invalidate.mock.calls.flatMap((call) => {
      const filters = call[0];
      if (filters?.queryKey) return [JSON.stringify(filters.queryKey)];
      if (filters?.predicate) {
        return mailKeys
          .filter((queryKey) => filters.predicate?.({ queryKey } as never))
          .map((queryKey) => JSON.stringify(queryKey));
      }
      return [];
    });
  const invalidatedEverything = () => invalidate.mock.calls.some((call) => call[0] === undefined);
  return { emit, invalidatedKeys, invalidatedEverything };
}

describe("useDaemonEventInvalidation", () => {
  beforeEach(() => {
    events.handler = undefined;
    toastMock.error.mockClear();
    useConnectionStore.setState({
      state: "offline",
      errorMessage: undefined,
      lastErrorAt: undefined,
    });
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  test("SyncError surfaces the error and records it on the connection store", () => {
    const { emit, invalidatedKeys } = setup();
    emit({ type: "SyncError", account_id: "acct-1", error: "imap handshake failed" });

    expect(toastMock.error).toHaveBeenCalledWith("Sync failed: imap handshake failed");
    const store = useConnectionStore.getState();
    expect(store.errorMessage).toBe("imap handshake failed");
    expect(store.lastErrorAt).toBeGreaterThan(0);
    expect(invalidatedKeys()).toContain(JSON.stringify(["shell"]));
  });

  test("ReminderTriggered refreshes the reply queue and mailbox", () => {
    const { emit, invalidatedKeys } = setup();
    emit({ type: "ReminderTriggered", sent_message_id: "msg-1" });

    const keys = invalidatedKeys();
    expect(keys).toContain(JSON.stringify(["reply-queue"]));
    expect(keys).toContain(JSON.stringify(["mailbox"]));
    expect(toastMock.error).not.toHaveBeenCalled();
  });

  test("MutationReconciliationFailed rolls back the affected surfaces and warns", () => {
    const { emit, invalidatedKeys } = setup();
    emit({
      type: "MutationReconciliationFailed",
      client_correlation_id: "corr-1",
      error_summary: "Gmail rejected the archive",
    });

    const keys = invalidatedKeys();
    expect(keys).toContain(JSON.stringify(["mailbox"]));
    expect(keys).toContain(JSON.stringify(["thread"]));
    expect(keys).toContain(JSON.stringify(["search"]));
    expect(keys).not.toContain(JSON.stringify(["settings"]));
    expect(toastMock.error).toHaveBeenCalledWith("Action didn't stick: Gmail rejected the archive");
  });

  test("EventsLagged invalidates every query for a full resync", () => {
    const { emit, invalidatedEverything } = setup();
    emit({ type: "EventsLagged", skipped: 512 });

    // A keyless invalidateQueries() invalidates the whole cache.
    expect(invalidatedEverything()).toBe(true);
    expect(toastMock.error).not.toHaveBeenCalled();
  });
});
