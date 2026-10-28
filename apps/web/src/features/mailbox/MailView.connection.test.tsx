import { QueryClient, QueryClientProvider, onlineManager } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { MailView } from "./MailView";
import { mailboxKey, shellKey } from "./api";
import type { MailboxResponse, MessageGroupView, ShellResponse } from "./types";
import { startDaemonAvailability, resetDaemonAvailabilityForTest } from "@/lib/daemonAvailability";
import { shouldRetryQuery } from "@/lib/queryClient";
import { useUiPrefs } from "@/state/uiPrefsStore";

vi.mock("@/lib/localHandshake", () => ({
  tryLocalHandshake: vi.fn<() => Promise<string>>(async () => "test-token"),
}));
vi.mock("@/lib/tokenStorage", () => ({
  clearToken: vi.fn<() => void>(),
  getBridgeBaseUrl: () => "http://mxr.test",
  getToken: () => "test-token",
}));
vi.mock("@/lib/ws", () => ({
  daemonEvents: {
    getStatus: () => ({ state: "connected" }),
    onStatus: () => () => {},
    reconnectNow: vi.fn<() => void>(),
  },
}));
vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: () => null,
    Navigate: () => null,
    Outlet: () => null,
    useNavigate: () => () => undefined,
    useParams: () => ({}),
  };
});
vi.mock("@/hooks/useMediaQuery", () => ({ SINGLE_PANE_QUERY: "", useMediaQuery: () => false }));
vi.mock("@/hooks/useSplitPane", () => ({
  useSplitPane: () => ({
    groupProps: {},
    sidePanelProps: {},
    handleProps: {},
    otherPanelProps: {},
  }),
}));
vi.mock("@/hooks/useAdvanceOnRemoval", () => ({
  useAdvanceOnRemoval: vi.fn<() => void>(),
  useRouteStillShows: () => () => true,
}));
vi.mock("@/lib/keys/focusPane", () => ({ focusActivePane: vi.fn<() => void>() }));
vi.mock("@/components/ModeFrame", () => {
  return {
    ModeFrame: ({ children }: { children: ReactNode }) => children,
    PageTitle: ({ children }: { children: string }) => <h1>{children}</h1>,
  };
});
vi.mock("@/components/ui/resizable", async () => {
  const React = await import("react");
  const Panel = ({ children }: { children: ReactNode }) =>
    React.createElement("div", null, children);
  return {
    ResizableHandle: () => null,
    ResizablePanel: Panel,
    ResizablePanelGroup: Panel,
  };
});
vi.mock("./MailboxList", async () => {
  const React = await import("react");
  return {
    MailboxList: ({ groups, empty }: { groups: MessageGroupView[]; empty: ReactNode }) =>
      groups.length > 0
        ? React.createElement(
            "div",
            null,
            groups.flatMap((group) =>
              group.rows.map((row) => React.createElement("p", { key: row.id }, row.subject)),
            ),
          )
        : empty,
  };
});

const EMPTY_MAILBOX: MailboxResponse = {
  mailbox: {
    lensLabel: "Inbox",
    view: "threads",
    counts: { total: 0 },
    has_more: false,
    next_offset: null,
    groups: [],
  },
};

const SHELL: ShellResponse = {
  sidebar: {
    sections: [
      {
        id: "system",
        title: "System",
        items: [{ id: "inbox", label: "Inbox", lens: { kind: "inbox" } }],
      },
    ],
  },
};

let mailboxBody: MailboxResponse;
let daemonDown: boolean;
let holdProbe: boolean;
let releaseProbe: (() => void) | null;
let queryClient: QueryClient;
let stopAvailability: (() => void) | undefined;

function apiResponse<T>(value: T, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const daemonConnectError = () =>
  apiResponse(
    { code: "connect", error: "failed to connect to mxr daemon: No such file or directory" },
    503,
  );

function createClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: shouldRetryQuery, retryDelay: 1_000, staleTime: 0 },
      mutations: { retry: false },
    },
  });
}

function renderInbox() {
  stopAvailability = startDaemonAvailability(queryClient);
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <MailView route={{ kind: "system", mailbox: "inbox" }} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

async function waitForPausedColdQueries(): Promise<void> {
  await waitFor(() => {
    expect(releaseProbe).not.toBeNull();
  });
  releaseProbe?.();
  releaseProbe = null;
  await waitFor(
    () => {
      const queries = queryClient.getQueryCache().findAll();
      expect(queries.some((query) => query.state.fetchStatus === "paused")).toBe(true);
    },
    { timeout: 5_000 },
  );
}

beforeEach(() => {
  mailboxBody = EMPTY_MAILBOX;
  daemonDown = true;
  holdProbe = true;
  releaseProbe = null;
  onlineManager.setOnline(true);
  resetDaemonAvailabilityForTest();
  queryClient = createClient();
  useUiPrefs.setState({ accountScope: null, listMode: "threads" });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const requestUrl = input;
      const path = new URL(requestUrl).pathname;
      if (path === "/api/v1/admin/status") {
        // Hold only the first probe. A later one (the backoff retry) held the
        // same way would never settle, and every probe after it would wait on
        // it, so recovery could never be seen.
        if (daemonDown && holdProbe) {
          holdProbe = false;
          return new Promise<Response>((resolve) => {
            releaseProbe = () => resolve(daemonConnectError());
          });
        }
        return daemonDown ? daemonConnectError() : apiResponse({ daemon_version: "0.6.69" });
      }
      if (daemonDown) return daemonConnectError();
      if (path === "/api/v1/client/shell") return apiResponse(SHELL);
      if (path === "/api/v1/mail/mailbox") return apiResponse(mailboxBody);
      throw new Error(`Unexpected request ${path}`);
    }),
  );
});

afterEach(() => {
  stopAvailability?.();
  stopAvailability = undefined;
  resetDaemonAvailabilityForTest();
  onlineManager.setOnline(true);
  queryClient.clear();
  vi.unstubAllGlobals();
});

describe("cold mailbox connection state", () => {
  test("a real connect 503 pauses a cold Inbox instead of rendering it empty, then recovers without reload", async () => {
    renderInbox();

    await waitForPausedColdQueries();
    // Query state can pause before React finishes the minimum skeleton display.
    expect(await screen.findByText("mxr isn't running", {}, { timeout: 5_000 })).toBeVisible();
    expect(screen.getByText(/nothing from this page was loaded/i)).toBeVisible();
    expect(screen.queryByText("Inbox zero")).not.toBeInTheDocument();

    daemonDown = false;
    window.dispatchEvent(new Event("focus"));

    expect(await screen.findByText("Inbox zero", {}, { timeout: 5_000 })).toBeVisible();
    expect(screen.queryByText("mxr isn't running")).not.toBeInTheDocument();
  });

  test("a cached message remains readable while the mailbox refetch is paused", async () => {
    const params = {
      lens_kind: "inbox" as const,
      view: "threads" as const,
      limit: 150,
      account: null,
    };
    const cached: MailboxResponse = {
      ...EMPTY_MAILBOX,
      mailbox: {
        ...EMPTY_MAILBOX.mailbox,
        counts: { total: 1 },
        groups: [
          {
            id: "today",
            label: "Today",
            rows: [
              {
                id: "message-1",
                kind: "thread",
                thread_id: "thread-1",
                provider_id: "provider-1",
                sender: "Ada",
                subject: "Cached message stays readable",
                snippet: "Cached snippet",
                date: "2026-10-11T10:00:00Z",
                date_label: "Today",
                date_full: "Today",
                date_relative: "now",
                unread: false,
                starred: false,
                has_attachments: false,
              },
            ],
          },
        ],
      },
    };
    queryClient.setQueryData(shellKey, SHELL);
    queryClient.setQueryData(
      mailboxKey(params),
      { pages: [cached], pageParams: [0] },
      { updatedAt: 0 },
    );

    renderInbox();
    await waitForPausedColdQueries();

    expect(screen.getByText("Cached message stays readable")).toBeVisible();
    expect(screen.queryByText("mxr isn't running")).not.toBeInTheDocument();
    expect(screen.queryByText("Inbox zero")).not.toBeInTheDocument();
  });

  test("a list answered from the limited search says so above the rows", async () => {
    const notice =
      "Search is limited while the index is unavailable: subject, sender and preview only. Restart the daemon or run `mxr doctor --reindex`.";
    mailboxBody = {
      mailbox: {
        ...EMPTY_MAILBOX.mailbox,
        degraded: {
          reason: "index_stopped",
          searched_fields: ["subject", "sender", "preview"],
          notice,
        },
      },
    };
    daemonDown = false;

    renderInbox();

    expect(await screen.findByText(notice, {}, { timeout: 5_000 })).toBeVisible();
  });

  test("a list answered by the index shows no limited-search line", async () => {
    daemonDown = false;

    renderInbox();

    expect(await screen.findByText("Inbox zero", {}, { timeout: 5_000 })).toBeVisible();
    expect(screen.queryByText(/Search is limited/)).toBeNull();
  });

  test.each([
    ["label", { kind: "label" as const, name: "receipts" }],
    ["saved search", { kind: "saved" as const, slug: "weekly" }],
  ])("a failed shell lookup does not call a %s missing", async (_name, route) => {
    stopAvailability = startDaemonAvailability(queryClient);
    render(
      <QueryClientProvider client={queryClient}>
        <MailView route={route} />
      </QueryClientProvider>,
    );

    await waitForPausedColdQueries();

    expect(await screen.findByText("mxr isn't running", {}, { timeout: 5_000 })).toBeVisible();
    expect(screen.queryByText("No label by that name")).not.toBeInTheDocument();
    expect(screen.queryByText("No saved search by that name")).not.toBeInTheDocument();
  });
});
