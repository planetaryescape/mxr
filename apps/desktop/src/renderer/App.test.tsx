import {
  act,
  fireEvent,
  render as rtlRender,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BridgeState } from "../shared/types";
import App from "./App";
import { ThemeProvider } from "./lib/theme";
import {
  configureDesktopMockServer,
  getRecordedDesktopRequests,
} from "./test/desktopMockServer";

const initialMailbox = {
  mailbox: {
    lensLabel: "Inbox",
    view: "threads" as const,
    counts: { unread: 12, total: 144 },
    groups: [
      {
        id: "today",
        label: "Today",
        rows: [
          {
            id: "msg-1",
            kind: "thread" as const,
            thread_id: "thread-1",
            provider_id: "provider-1",
            sender: "Deploy Bot",
            sender_detail: "deploys@example.com",
            subject: "Deploy complete",
            snippet: "Production deploy succeeded in 42 seconds.",
            date_label: "Today",
            unread: true,
            starred: false,
            has_attachments: true,
            message_count: 1,
          },
        ],
      },
    ],
  },
  sidebar: {
    sections: [
      {
        id: "system",
        title: "System",
        items: [
          {
            id: "inbox",
            label: "Inbox",
            unread: 12,
            total: 144,
            active: true,
            lens: { kind: "inbox" as const },
          },
        ],
      },
    ],
  },
  shell: {
    accountLabel: "personal",
    syncLabel: "Synced",
    statusMessage: "Local-first and ready",
    commandHint: "Ctrl-p",
  },
};

const readyBridge = {
  kind: "ready" as const,
  baseUrl: "http://127.0.0.1:4010",
  authToken: "test-token",
  binaryPath: "/usr/local/bin/mxr",
  usingBundled: true,
  daemonVersion: "0.4.4",
  protocolVersion: 1,
  initialMailbox: null,
};

const defaultDesktopSettings = {
  theme: "mxr-dark" as const,
  keymapOverrides: {},
};

const mismatchBridge = {
  kind: "mismatch" as const,
  binaryPath: "/usr/local/bin/mxr",
  usingBundled: false,
  daemonVersion: "0.4.2",
  actualProtocol: 0,
  requiredProtocol: 1,
  updateSteps: [
    "Homebrew: brew upgrade mxr",
    "Release install: rerun ./install.sh",
    "Source install: git pull && cargo install --path crates/daemon --locked",
  ],
  detail:
    "mxr Desktop needs a compatible version of mxr before it can connect.",
};

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  readonly url: string;
  closed = false;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    this.closed = true;
  }

  simulateOpen() {
    this.onopen?.();
  }

  simulateMessage(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }
}

function installDesktopApi(bridgeState: BridgeState = readyBridge) {
  let desktopSettings = { ...defaultDesktopSettings };
  const api = {
    getBridgeState: vi.fn().mockResolvedValue(bridgeState),
    retryBridge: vi.fn(),
    useBundledMxr: vi.fn(),
    setExternalBinaryPath: vi.fn(),
    getDesktopSettings: vi.fn().mockImplementation(async () => desktopSettings),
    updateDesktopSettings: vi.fn().mockImplementation(async (patch) => {
      desktopSettings = {
        ...desktopSettings,
        ...patch,
        keymapOverrides: patch?.keymapOverrides ?? desktopSettings.keymapOverrides,
      };
      return desktopSettings;
    }),
    openDraftInEditor: vi.fn().mockResolvedValue({ ok: true }),
    pickAttachments: vi
      .fn()
      .mockResolvedValue({ paths: ["/tmp/deploy.log", "/tmp/screenshot.png"] }),
    openBrowserDocument: vi.fn().mockResolvedValue({ ok: true }),
    openExternalUrl: vi.fn().mockResolvedValue({ ok: true }),
    openLocalPath: vi.fn().mockResolvedValue({ ok: true }),
    openConfigFile: vi.fn().mockResolvedValue({ ok: true }),
  };
  Object.defineProperty(window, "mxrDesktop", {
    value: api,
    configurable: true,
  });
  return api;
}

function installFetchMocks(options?: {
  delayReadMutation?: Promise<void>;
  delayMailbox?: Promise<void>;
  delayMailboxLensKind?: string;
  sendFailureMessage?: string;
}) {
  configureDesktopMockServer(options);
  return {
    requests: getRecordedDesktopRequests,
  };
}

function readMutationCalls() {
  return getRecordedDesktopRequests().filter(
    (request) => request.path === "/mutations/read",
  );
}

function findRequest(path: string, method = "GET") {
  return getRecordedDesktopRequests().find(
    (request) => request.path === path && request.method === method,
  );
}

function findRequestMatching(
  predicate: (
    request: ReturnType<typeof getRecordedDesktopRequests>[number],
  ) => boolean,
) {
  return getRecordedDesktopRequests().find(predicate);
}

function parseRequestBody<T>(request: { body: string | null } | undefined) {
  if (!request?.body) {
    return null as T | null;
  }
  return JSON.parse(request.body) as T;
}

async function flushAsyncWork() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

function findActiveLens(label: string) {
  return screen.findByText((_content, element) =>
    element?.tagName === "H1" && element.textContent === label ? true : false,
  );
}

function getActiveLens(label: string) {
  return screen.getByText((_content, element) =>
    element?.tagName === "H1" && element.textContent === label ? true : false,
  );
}

function setNavigatorPlatform(platform: string) {
  Object.defineProperty(window.navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

function renderApp() {
  return rtlRender(
    <ThemeProvider>
      <App />
    </ThemeProvider>,
  );
}

describe("App", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    setNavigatorPlatform("Linux");
    MockWebSocket.instances = [];
    vi.stubGlobal("WebSocket", MockWebSocket);
    configureDesktopMockServer();
  });

  it("renders mismatch guidance with update steps", async () => {
    installDesktopApi(mismatchBridge);

    renderApp();

    expect(
      await screen.findByText("mxr Desktop needs a compatible version of mxr"),
    ).toBeInTheDocument();
    expect(screen.getByText("Homebrew: brew upgrade mxr")).toBeInTheDocument();
    expect(screen.getByText("Use bundled mxr")).toBeInTheDocument();
  });

  it("renders the dark workbench shell and switches screens", async () => {
    installDesktopApi();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });
    expect(screen.getByRole("button", { name: "Search" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rules" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Accounts" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Diagnostics" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Local-first and ready")).toBeInTheDocument();
    expect(screen.getByText("System")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Inbox/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Search" }));

    await waitFor(() => {
      expect(screen.getByRole("tab", { name: "Threads" })).toBeInTheDocument();
    });
    expect(screen.queryByText("System")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Threads" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Sort" })).toBeInTheDocument();
  });

  it("hydrates from the bridge snapshot without waiting on a second mailbox fetch", async () => {
    installDesktopApi({
      ...readyBridge,
      initialMailbox,
    });
    installFetchMocks();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });
    expect(getActiveLens("Inbox")).toBeInTheDocument();
    expect(screen.getByText("Local-first and ready")).toBeInTheDocument();
    expect(screen.queryByText("Loading local workspace")).not.toBeInTheDocument();
    expect(
      getRecordedDesktopRequests().filter((request) => request.path === "/mailbox"),
    ).toHaveLength(0);
  });

  it("opens the selected thread with the keyboard and closes back to two-pane", async () => {
    installDesktopApi();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    fireEvent.keyDown(window, { key: "Enter" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(
      screen.getAllByRole("button", { name: "Archive" }).length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "Escape" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(screen.queryAllByRole("button", { name: "Archive" })).toHaveLength(
      0,
    );
  });

  it("opens with o and scrolls the reader instead of moving the mail list selection", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");
    expect(screen.getAllByTestId("mail-row")[0]?.className).toContain(
      "bg-panel-elevated",
    );

    fireEvent.keyDown(window, { key: "o" });

    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    const readerRegions = screen.getAllByTestId("reader-scroll-region");
    for (const region of readerRegions) {
      Object.defineProperty(region, "scrollTop", {
        value: 0,
        writable: true,
        configurable: true,
      });
    }

    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(
        readerRegions.every((region) => (region as HTMLDivElement).scrollTop > 0),
      ).toBe(true);
    });

    expect(screen.getAllByTestId("mail-row")[0]?.className).toContain(
      "bg-panel-elevated",
    );
    expect(
      screen.getAllByRole("heading", { name: "Deploy complete" }).length,
    ).toBeGreaterThan(0);
  });

  it("wires the live event stream and refreshes the mailbox on sync completion", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(
      getRecordedDesktopRequests().filter(
        (request) => request.path === "/mailbox",
      ),
    ).toHaveLength(1);

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      MockWebSocket.instances[0].simulateMessage({
        event: "SyncCompleted",
        account_id: "personal",
        messages_synced: 3,
      });
    });

    await waitFor(() => {
      expect(
        getRecordedDesktopRequests().filter(
          (request) => request.path === "/mailbox",
        ),
      ).toHaveLength(2);
    });
  });

  it("switches mailbox lenses from the sidebar", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: /All Mail/i }));

    await waitFor(() => {
      expect(getActiveLens("All Mail")).toBeInTheDocument();
    });
    expect(screen.getAllByText("Review requested").length).toBeGreaterThan(0);
  });

  it("shows a loading indicator while opening a slower sidebar lens", async () => {
    installDesktopApi();
    let resolveMailbox: (() => void) | undefined;
    const delayedMailbox = new Promise<void>((resolve) => {
      resolveMailbox = () => resolve();
    });
    installFetchMocks({
      delayMailbox: delayedMailbox,
      delayMailboxLensKind: "all_mail",
    });

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: /All Mail/i }));

    expect(
      await screen.findAllByText("Loading All Mail..."),
    ).not.toHaveLength(0);

    resolveMailbox?.();

    await waitFor(() => {
      expect(screen.queryByText("Loading All Mail...")).not.toBeInTheDocument();
    });
    await waitFor(() => {
      expect(getActiveLens("All Mail")).toBeInTheDocument();
    });
  });

  it("shows the mac command hint and opens the command palette from the app shortcut event", async () => {
    setNavigatorPlatform("MacIntel");
    installDesktopApi();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });
    expect(document.body.textContent).toContain("⌘P");

    fireEvent.keyDown(window, { key: "2" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(document.body.textContent).toContain("Search");

    window.dispatchEvent(new CustomEvent("mxr:command-palette"));
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "g" });
    fireEvent.keyDown(window, { key: "i" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(
      screen.queryByRole("tab", { name: "Threads" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mailbox" })).toBeInTheDocument();
  });

  it("selects the first filtered command and runs it on enter", async () => {
    installDesktopApi();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });

    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "search" },
    });

    await waitFor(() => {
      expect(document.querySelector('[aria-selected="true"]')).not.toBeNull();
    });

    fireEvent.keyDown(window, { key: "Enter" });
    await act(async () => {
      await flushAsyncWork();
    });

    expect(screen.getByRole("tab", { name: "Threads" })).toBeInTheDocument();
  });

  it("moves the command palette selection with j", async () => {
    installDesktopApi();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(document.querySelector('[aria-selected="true"]')).not.toBeNull();
    });
    const firstSelectedText = document.querySelector(
      '[aria-selected="true"]',
    )?.textContent;

    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      const selected = document.querySelector('[aria-selected="true"]');
      expect(selected).not.toBeNull();
      expect(selected?.textContent).not.toBe(firstSelectedText);
    });
  });

  it("keeps the mailbox selection in view while navigating with j", async () => {
    installDesktopApi();
    const scrollSpy = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      value: scrollSpy,
      configurable: true,
    });

    renderApp();

    await findActiveLens("Inbox");
    scrollSpy.mockClear();

    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(scrollSpy).toHaveBeenCalledWith({ block: "nearest" });
    });
  });

  it("extends a continuous selection in visual mode while moving with j", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "V" });
    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(screen.getByText("Vercel").closest("button")?.className).toContain(
        "bg-success",
      );
    });
    expect(screen.getByText("Stripe").closest("button")?.className).toContain(
      "bg-panel-elevated",
    );
  });

  it("keeps the search selection in view while navigating with j", async () => {
    installDesktopApi();
    const scrollSpy = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      value: scrollSpy,
      configurable: true,
    });

    renderApp();

    await screen.findByRole("button", { name: "Search" });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByRole("tab", { name: "Threads" });
    scrollSpy.mockClear();

    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(scrollSpy).toHaveBeenCalledWith({ block: "nearest" });
    });
  });

  it("moves focus to the sidebar with h from the mail list", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "h" });
    fireEvent.keyDown(window, { key: "j" });

    expect(getActiveLens("Inbox")).toBeInTheDocument();
  });

  it("opens the selected sidebar lens with o while a reader is open", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(
      (await screen.findAllByRole("button", { name: "Archive" })).length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "h" });
    fireEvent.keyDown(window, { key: "j" });

    expect(getActiveLens("Inbox")).toBeInTheDocument();

    const requestCountBeforeOpen = getRecordedDesktopRequests().filter(
      (request) => request.path === "/mailbox",
    ).length;

    fireEvent.keyDown(window, { key: "o" });

    await waitFor(() => {
      expect(getActiveLens("All Mail")).toBeInTheDocument();
    });
    expect(
      getRecordedDesktopRequests().filter((request) => request.path === "/mailbox")
        .length,
    ).toBe(requestCountBeforeOpen + 1);

    fireEvent.keyDown(window, { key: "j" });

    await act(async () => {
      await flushAsyncWork();
    });

    expect(
      getRecordedDesktopRequests().filter((request) => request.path === "/mailbox")
        .length,
    ).toBe(requestCountBeforeOpen + 1);
  });

  it("opens the selected sidebar lens with l while a reader is open", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(
      (await screen.findAllByRole("button", { name: "Archive" })).length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "h" });
    fireEvent.keyDown(window, { key: "j" });

    expect(getActiveLens("Inbox")).toBeInTheDocument();

    const requestCountBeforeOpen = getRecordedDesktopRequests().filter(
      (request) => request.path === "/mailbox",
    ).length;

    fireEvent.keyDown(window, { key: "l" });

    await waitFor(() => {
      expect(getActiveLens("All Mail")).toBeInTheDocument();
    });
    expect(
      getRecordedDesktopRequests().filter((request) => request.path === "/mailbox")
        .length,
    ).toBe(requestCountBeforeOpen + 1);

    fireEvent.keyDown(window, { key: "j" });

    await act(async () => {
      await flushAsyncWork();
    });

    expect(
      getRecordedDesktopRequests().filter((request) => request.path === "/mailbox")
        .length,
    ).toBe(requestCountBeforeOpen + 1);
  });

  it("dispatches manifest-driven star mutations from the mail list", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "s" });

    await waitFor(() => {
      expect(findRequest("/mutations/star", "POST")).toBeDefined();
    });
  });

  it("optimistically marks messages read and shows a visible sync indicator", async () => {
    installDesktopApi();
    let resolveRead!: () => void;
    const delayedRead = new Promise<void>((resolve) => {
      resolveRead = resolve;
    });
    installFetchMocks({ delayReadMutation: delayedRead });

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "I" });

    // Unread count decrements optimistically
    expect(screen.getByText("Marking 1 message read")).toBeInTheDocument();
    expect(screen.getAllByText("Syncing").length).toBeGreaterThan(0);

    await waitFor(() => {
      expect(
        parseRequestBody<{ message_ids: string[]; read: boolean }>(
          findRequest("/mutations/read", "POST"),
        ),
      ).toEqual({
        message_ids: ["msg-1"],
        read: true,
      });
    });

    resolveRead();

    await waitFor(() => {
      expect(
        screen.queryByText("Marking 1 message read"),
      ).not.toBeInTheDocument();
    });
  });

  it("delays preview mark-read until the reader settles on one message for five seconds", async () => {
    vi.useFakeTimers();
    installDesktopApi();
    installFetchMocks();

    try {
      renderApp();
      await act(async () => {
        await flushAsyncWork();
      });
      expect(getActiveLens("Inbox")).toBeInTheDocument();

      fireEvent.keyDown(window, { key: "Enter" });
      await act(async () => {
        await flushAsyncWork();
      });
      expect(findRequest("/thread/thread-1")).toBeDefined();

      fireEvent.keyDown(window, { key: "j" });
      await act(async () => {
        await flushAsyncWork();
      });

      expect(readMutationCalls()).toHaveLength(0);

      await vi.advanceTimersByTimeAsync(4900);

      expect(readMutationCalls()).toHaveLength(0);

      await vi.advanceTimersByTimeAsync(100);
      await flushAsyncWork();

      expect(readMutationCalls()).toHaveLength(1);
      expect(
        parseRequestBody<{ message_ids: string[]; read: boolean }>(
          readMutationCalls()[0],
        ),
      ).toEqual({
        message_ids: ["msg-2"],
        read: true,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps the current mailbox selection after a mailbox refresh", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();
    await act(async () => {
      await flushAsyncWork();
    });
    expect(getActiveLens("Inbox")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "j" });
    expect(screen.getByText("Billing alert").closest("button")?.className).toContain(
      "border-l-accent",
    );

    act(() => {
      MockWebSocket.instances[0]?.simulateOpen();
      MockWebSocket.instances[0]?.simulateMessage({
        event: "SyncCompleted",
        account_id: "personal",
        messages_synced: 1,
      });
    });

    await waitFor(() => {
      expect(
        getRecordedDesktopRequests().filter(
          (request) => request.path === "/mailbox",
        ).length,
      ).toBeGreaterThan(1);
    });
    expect(screen.getByText("Billing alert").closest("button")?.className).toContain(
      "border-l-accent",
    );
  });

  it("opens compose, launches the editor, and sends the draft", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "c" });

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();

    // Step 1: recipient picker shown with contact list
    expect(
      screen.getByPlaceholderText("Type a name or email..."),
    ).toBeInTheDocument();
  });

  it("opens a reply shell for the selected message", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "r" });

    expect(
      await screen.findByRole("heading", { name: "Reply" }),
    ).toBeInTheDocument();

    const composeCall = findRequest("/compose/session", "POST");
    expect(composeCall).toBeDefined();
    expect(
      parseRequestBody<{ kind: string; message_id: string }>(composeCall),
    ).toMatchObject({
      kind: "reply",
      message_id: "msg-1",
    });
    // Step 1: recipients shown with Write body button
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
    });
  });

  it("adds and removes compose attachments before sending", async () => {
    const desktopApi = installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "c" });

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Attach files" }));

    await waitFor(() => {
      expect(desktopApi.pickAttachments).toHaveBeenCalled();
    });
    expect(screen.getByText("deploy.log")).toBeInTheDocument();
    expect(screen.getByText("screenshot.png")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "Remove attachment deploy.log" }),
    );
    expect(screen.queryByText("deploy.log")).not.toBeInTheDocument();

    const toInput = screen.getByPlaceholderText("Type a name or email...");
    fireEvent.change(toInput, { target: { value: "teammate@example.com" } });
    fireEvent.keyDown(toInput, { key: "Enter" });

    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      const updateRequest = findRequest("/compose/session/update", "POST");
      expect(updateRequest).toBeDefined();
      expect(
        parseRequestBody<{ attach: string[] }>(updateRequest)?.attach,
      ).toEqual(["/tmp/screenshot.png"]);
    });
  });

  it("commits a typed recipient before sending even without pressing enter", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "c" });

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Type a name or email..."), {
      target: { value: "typed@example.com" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(
        parseRequestBody<{ to: string }>(
          findRequest("/compose/session/update", "POST"),
        )?.to,
      ).toBe("typed@example.com");
    });
  });

  it("does not run a delayed autosave update after send has started", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "c" });

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();

    const toInput = screen.getByPlaceholderText("Type a name or email...");
    fireEvent.change(toInput, { target: { value: "race@example.com" } });
    fireEvent.keyDown(toInput, { key: "Enter" });

    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await act(async () => {
      await flushAsyncWork();
      await new Promise((resolve) => window.setTimeout(resolve, 300));
      await flushAsyncWork();
      await flushAsyncWork();
    });

    expect(
      getRecordedDesktopRequests().filter(
        (request) =>
          request.path === "/compose/session/update" &&
          request.method === "POST",
      ),
    ).toHaveLength(1);
    expect(
      getRecordedDesktopRequests().filter(
        (request) =>
          request.path === "/compose/session/send" &&
          request.method === "POST",
      ),
    ).toHaveLength(1);
  });

  it("keeps compose open and shows keychain repair guidance when send fails", async () => {
    installDesktopApi();
    installFetchMocks({
      sendFailureMessage:
        "ipc error: Provider error: Keyring error: Password for mxr/consulting-smtp/hello@bhekani.com requires interactive macOS keychain approval.",
    });

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "c" });

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Type a name or email..."), {
      target: { value: "client@example.com" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(
      await screen.findByText((content) =>
        content.includes("mxr accounts repair consulting"),
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();
    expect(
      getRecordedDesktopRequests().filter(
        (request) =>
          request.path === "/compose/session/send" &&
          request.method === "POST",
      ),
    ).toHaveLength(1);
  });

  it("resumes the most recent saved draft from the header", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    const resumeButton = await screen.findByRole("button", {
      name: "Resume draft",
    });
    fireEvent.click(resumeButton);

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(findRequest("/compose/session/restore", "POST")).toBeDefined();
    });
    expect(screen.getByDisplayValue("Recovered draft")).toBeInTheDocument();
    expect(screen.getByText("wireframes.png")).toBeInTheDocument();
  });

  it("applies labels and moves the selected message", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Label" }));
    await act(async () => {
      await flushAsyncWork();
    });
    expect(document.body.textContent).toContain("Apply label");
    fireEvent.click(screen.getByRole("checkbox", { name: /follow up/i }));
    expect(screen.getByRole("button", { name: "Apply" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await act(async () => {
      await flushAsyncWork();
    });
    fireEvent.click(screen.getByRole("button", { name: /Move To Label/i }));

    await act(async () => {
      await flushAsyncWork();
    });
    expect(document.body.textContent).toContain("Move message");
    expect(screen.getByLabelText("Target")).toHaveValue("Inbox");
    expect(
      screen.getByRole("option", { name: "Follow Up" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Move" })).toBeInTheDocument();
  });

  it("navigates the label dialog with j/k, toggles with space, and submits with enter", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Label" }));

    const dialog = await screen.findByRole("dialog");
    const customInput = screen.getByPlaceholderText("Follow Up, Waiting");
    const inboxOption = screen.getByRole("checkbox", { name: /inbox/i });
    const waitingOption = screen.getByRole("checkbox", { name: /waiting/i });
    await waitFor(() => {
      expect(document.activeElement).not.toBe(customInput);
      expect(inboxOption).toHaveAttribute("data-active", "true");
    });
    expect(document.body.textContent).toContain("j/k move");
    expect(document.body.textContent).toContain("space toggle");
    expect(document.body.textContent).toContain("enter apply");
    expect(document.body.textContent).toContain("tab custom labels");
    expect(document.body.textContent).toContain("esc close");

    fireEvent.keyDown(dialog, { key: "j" });
    fireEvent.keyDown(dialog, { key: "k" });
    fireEvent.keyDown(dialog, { key: "j" });
    fireEvent.keyDown(dialog, { key: "j" });
    fireEvent.keyDown(dialog, { key: "j" });
    expect(waitingOption).toHaveAttribute("data-active", "true");
    fireEvent.keyDown(dialog, { key: " " });
    fireEvent.keyDown(dialog, { key: "Enter" });

    await waitFor(() => {
      const request = findRequest("/mutations/labels", "POST");
      expect(request).toBeDefined();
      expect(
        parseRequestBody<{ add: string[] }>(request)?.add,
      ).toEqual(["Waiting"]);
    });
  });

  it("opens the label dialog from the reader with l", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "o" });
    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    expect(document.body.textContent).toContain("lLabel");

    fireEvent.keyDown(window, { key: "l" });

    await waitFor(() => {
      expect(document.body.textContent).toContain("Apply label");
    });
    expect(screen.getByRole("button", { name: "Apply" })).toBeInTheDocument();
  });

  it("supports richer search controls plus snooze and unsubscribe flows", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByRole("tab", { name: "Threads" });

    fireEvent.change(screen.getByRole("combobox", { name: "Search mode" }), {
      target: { value: "semantic" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "Sort" }), {
      target: { value: "recent" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: "Explain" }));
    fireEvent.change(
      screen.getByPlaceholderText("Search subjects, senders, snippets"),
      {
        target: { value: "deploy" },
      },
    );

    await act(async () => {
      await flushAsyncWork();
    });
    const searchCall = findRequestMatching(
      (request) =>
        request.path === "/search" &&
        request.url.includes("mode=semantic") &&
        request.url.includes("sort=recent") &&
        request.url.includes("explain=true") &&
        request.url.includes("q=deploy"),
    );
    expect(searchCall).toBeDefined();
    expect(document.body.textContent).toContain("semantic");
    expect(document.body.textContent).toContain('"query": "deploy"');
    expect(await screen.findByText("Deploy complete")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "Z" });

    expect(
      await screen.findByRole("heading", { name: "Snooze message" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Snooze" }));

    await act(async () => {
      await flushAsyncWork();
    });
    expect(findRequest("/actions/snooze", "POST")).toBeDefined();

    fireEvent.keyDown(window, { key: "D" });

    expect(
      await screen.findByRole("heading", { name: "Unsubscribe" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Unsubscribe" }));

    await act(async () => {
      await flushAsyncWork();
    });
    expect(findRequest("/actions/unsubscribe", "POST")).toBeDefined();
  });

  it("leaves the search input and returns to keyboard navigation on enter", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    const input = await screen.findByPlaceholderText(
      "Search subjects, senders, snippets",
    );

    fireEvent.change(input, {
      target: { value: "deploy" },
    });
    await screen.findByText("Deploy complete");

    expect(document.activeElement).toBe(input);

    fireEvent.keyDown(input, { key: "Enter" });
    expect(document.activeElement).not.toBe(input);

    fireEvent.keyDown(window, { key: "j" });
    expect((input as HTMLInputElement).value).toBe("deploy");
  });

  it("opens the search workspace from / and focuses the search input", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "/" });

    const input = await screen.findByPlaceholderText(
      "Search subjects, senders, snippets",
    );
    expect(screen.getByRole("tab", { name: "Threads" })).toBeInTheDocument();
    expect(document.activeElement).toBe(input);
  });

  it("opens the search workspace from / while a reader is already open", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "o" });
    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "/" });

    const input = await screen.findByPlaceholderText(
      "Search subjects, senders, snippets",
    );
    expect(screen.getByRole("tab", { name: "Threads" })).toBeInTheDocument();
    expect(document.activeElement).toBe(input);
  });

  it("opens and closes the search reader with the same lifecycle as mailbox", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    const input = await screen.findByPlaceholderText(
      "Search subjects, senders, snippets",
    );

    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(window, { key: "Enter" });

    expect(
      (await screen.findAllByRole("button", { name: "Archive" })).length,
    ).toBeGreaterThan(0);

    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]!);

    await waitFor(() => {
      expect(screen.queryAllByRole("button", { name: "Archive" })).toHaveLength(
        0,
      );
    });
  });

  it("archives from the search reader", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    const input = await screen.findByPlaceholderText(
      "Search subjects, senders, snippets",
    );

    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(window, { key: "Enter" });

    fireEvent.click(
      (await screen.findAllByRole("button", { name: "Archive" }))[0]!,
    );

    await waitFor(() => {
      expect(findRequest("/mutations/archive", "POST")).toBeDefined();
    });
  });

  it("shows reader-mode shortcut hints while reading from search", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    const input = await screen.findByPlaceholderText(
      "Search subjects, senders, snippets",
    );

    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(window, { key: "o" });

    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);
    expect(screen.getByText("Reading View")).toBeInTheDocument();
    expect(screen.getAllByText("Reply").length).toBeGreaterThan(0);
  });

  it("opens the mailbox filter from Ctrl-f", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "f", ctrlKey: true });

    const input = await screen.findByPlaceholderText(
      "Filter by sender, subject, snippet...",
    );
    expect(document.activeElement).toBe(input);
  });

  it("switches the mailbox between thread and message views", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    expect(screen.getAllByTestId("mail-row")).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: "Messages" }));

    await waitFor(() => {
      expect(
        findRequestMatching(
          (request) =>
            request.path === "/mailbox" &&
            request.url.includes("view=messages"),
        ),
      ).toBeDefined();
    });
    await waitFor(() => {
      expect(screen.getAllByTestId("mail-row")).toHaveLength(3);
    });
  });

  it("keeps moving forward through message rows instead of jumping back to the top", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Messages" }));
    await screen.findByText("Deploy follow-up");

    fireEvent.keyDown(window, { key: "j" });
    expect(screen.getByText("Billing alert").closest("button")?.className).toContain(
      "bg-panel-elevated",
    );

    fireEvent.keyDown(window, { key: "j" });
    expect(
      screen.getByText("Deploy follow-up").closest("button")?.className,
    ).toContain("border-l-accent");
    expect(
      screen.getByText("Deploy complete").closest("button")?.className,
    ).not.toContain("border-l-accent");
  });

  it("updates the open reader as mailbox selection moves", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "j" });

    expect(
      (await screen.findAllByRole("heading", { name: "Billing alert" })).length,
    ).toBeGreaterThan(0);
  });

  it("opens the currently selected mailbox row after returning from reader focus", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "o" });
    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "h" });
    fireEvent.keyDown(window, { key: "j" });
    fireEvent.keyDown(window, { key: "o" });

    expect(
      (await screen.findAllByRole("heading", { name: "Billing alert" })).length,
    ).toBeGreaterThan(0);
  });

  it("toggles HTML reader mode with H", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });

    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "H" });

    expect(
      (await screen.findAllByTitle("HTML message")).length,
    ).toBeGreaterThan(0);
  });

  it("renders attachment-native search rows with open and download actions", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByRole("tab", { name: "Threads" });

    fireEvent.click(screen.getByRole("tab", { name: "Attachments" }));

    expect((await screen.findAllByText("deploy.log")).length).toBeGreaterThan(
      0,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Open attachment deploy.log" }),
    );
    await waitFor(() => {
      expect(findRequest("/attachments/open", "POST")).toBeDefined();
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Download attachment deploy.log" }),
    );
    await waitFor(() => {
      expect(findRequest("/attachments/download", "POST")).toBeDefined();
    });
  });

  it("opens the start-here onboarding from the command palette", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "Start Here" },
    });
    fireEvent.keyDown(window, { key: "Enter" });

    expect(
      await screen.findByRole("heading", { name: "Start here" }),
    ).toBeInTheDocument();
  });

  it("surfaces diagnostics operations for drafts, subscriptions, snoozed mail, and semantic reindex", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));

    expect(
      await screen.findByRole("tab", { name: /Drafts/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: /Subscriptions/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Snoozed/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Subscriptions/i }));
    expect(await screen.findByText("Vercel")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Snoozed/i }));
    expect(await screen.findByText("Billing alert")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Semantic/i }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Reindex semantic" }),
    );
    await waitFor(() => {
      expect(findRequest("/semantic/reindex", "POST")).toBeDefined();
    });

    fireEvent.click(screen.getByRole("tab", { name: /Drafts/i }));
    expect(await screen.findByText("Saved drafts")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    await waitFor(() => {
      expect(findRequest("/compose/session/restore", "POST")).toBeDefined();
    });
  });

  it("creates, renames, and deletes labels plus deletes saved searches from diagnostics", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));
    fireEvent.click(await screen.findByRole("tab", { name: /Labels/i }));
    expect(
      await screen.findByPlaceholderText("Create label"),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Create label"), {
      target: { value: "Escalations" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => {
      expect(findRequest("/labels/create", "POST")).toBeDefined();
    });

    fireEvent.click(screen.getAllByRole("button", { name: "Rename" })[0]!);
    fireEvent.change(screen.getByDisplayValue("Follow Up"), {
      target: { value: "Priority" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(findRequest("/labels/rename", "POST")).toBeDefined();
    });

    fireEvent.click(screen.getAllByRole("button", { name: "Delete" })[0]!);
    await waitFor(() => {
      expect(findRequest("/labels/delete", "POST")).toBeDefined();
    });

    fireEvent.click(screen.getByRole("tab", { name: /Saved Searches/i }));
    fireEvent.click(
      (await screen.findAllByRole("button", { name: "Delete" }))[0]!,
    );
    await waitFor(() => {
      expect(findRequest("/saved-searches/delete", "POST")).toBeDefined();
    });
  });

  it("opens a diagnostics sub-workspace from the command palette", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "Open diagnostics labels" },
    });
    fireEvent.keyDown(window, { key: "Enter" });

    expect(
      await screen.findByPlaceholderText("Create label"),
    ).toBeInTheDocument();
  });

  it("opens diagnostics settings from the keyboard and persists the selected theme", async () => {
    const desktopApi = installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "5" });
    await screen.findByRole("tab", { name: /Drafts/i });

    fireEvent.keyDown(window, { key: "T" });

    const themeSelect = await screen.findByLabelText("Theme");
    fireEvent.change(themeSelect, {
      target: { value: "catppuccin-mocha" },
    });

    await waitFor(() => {
      expect(desktopApi.updateDesktopSettings).toHaveBeenCalledWith({
        theme: "catppuccin-mocha",
      });
      expect(document.documentElement).toHaveAttribute(
        "data-theme",
        "catppuccin-mocha",
      );
    });
  });

  it("applies keymap overrides from settings to both visible labels and keyboard actions", async () => {
    const desktopApi = installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "5" });
    await screen.findByRole("tab", { name: /Drafts/i });

    fireEvent.keyDown(window, { key: "T" });

    fireEvent.change(await screen.findByLabelText("Keymap JSON"), {
      target: {
        value: `{
  // dev override
  "mailList": {
    "Ctrl-k": "compose"
  }
}`,
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save keymap" }));

    await waitFor(() => {
      expect(desktopApi.updateDesktopSettings).toHaveBeenCalledWith({
        keymapOverrides: {
          mailList: {
            "Ctrl-k": "compose",
          },
        },
      });
    });
    expect(screen.getByRole("button", { name: "Compose" }).textContent).toContain(
      "Ctrl-k",
    );

    fireEvent.keyDown(window, { key: "1" });
    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });

    expect(
      await screen.findByRole("heading", { name: "New message" }),
    ).toBeInTheDocument();
  });

  it("toggles remote content in the active HTML reader", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "H" });
    expect(
      (await screen.findAllByRole("button", {
        name: "Load remote content (M)",
      })).length,
    ).toBeGreaterThan(0);

    const htmlFrames = await screen.findAllByTitle("HTML message");
    await waitFor(() => {
      expect(
        htmlFrames.every(
          (frame) =>
            !(frame.getAttribute("srcdoc") ?? "").includes(
              "https://cdn.example.com/deploy.png",
            ),
        ),
      ).toBe(true);
    });

    fireEvent.keyDown(window, { key: "M" });

    await waitFor(() => {
      expect(
        screen
          .getAllByTitle("HTML message")
          .every((frame) =>
            (frame.getAttribute("srcdoc") ?? "").includes(
              "https://cdn.example.com/deploy.png",
            ),
          ),
      ).toBe(true);
    });
  });

  it("sizes the HTML reader frame to its loaded content", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "o" });
    expect(
      (await screen.findAllByRole("heading", { name: "Deploy complete" }))
        .length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "H" });

    const [frame] = await screen.findAllByTitle("HTML message");
    Object.defineProperty(frame, "contentDocument", {
      configurable: true,
      value: {
        body: { scrollHeight: 960, offsetHeight: 960 },
        documentElement: { scrollHeight: 960, offsetHeight: 960 },
      },
    });

    fireEvent.load(frame);

    await waitFor(() => {
      expect((frame as HTMLIFrameElement).style.height).toBe("960px");
    });
    expect((frame as HTMLIFrameElement).getAttribute("srcdoc")).toContain(
      "max-width: 100%",
    );
  });

  it("opens a rendered browser document, exports threads, and opens attachment/link dialogs", async () => {
    const desktopApi = installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(
      (await screen.findAllByRole("button", { name: "Archive" })).length,
    ).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "O" });

    await waitFor(() => {
      expect(desktopApi.openBrowserDocument).toHaveBeenCalledWith(
        expect.objectContaining({
          title: "Deploy complete",
          html: expect.stringContaining("Production deploy succeeded"),
        }),
      );
    });

    fireEvent.keyDown(window, { key: "A" });
    expect(
      await screen.findByRole("heading", { name: "Attachments" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => {
      expect(findRequest("/attachments/open", "POST")).toBeDefined();
    });

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "E" });

    expect(
      await screen.findByRole("heading", { name: "Thread export" }),
    ).toBeInTheDocument();
    expect(await screen.findByText(/Export body/)).toBeInTheDocument();
  });

  it("loads rules and accounts workspaces with real actions", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Rules" }));
    expect(
      await screen.findByRole("heading", { name: "Rules" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Archive receipts")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "History" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dry run" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(
      await screen.findByRole("heading", { name: "New rule" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    expect(
      await screen.findByRole("heading", { name: "Accounts" }),
    ).toBeInTheDocument();
    expect((await screen.findAllByText("Personal")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Test" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Set default" }),
    ).toBeInTheDocument();
  });

  it("opens a new rule from the keyboard on the rules screen", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Rules" }));
    expect(
      await screen.findByRole("heading", { name: "Rules" }),
    ).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "n" });

    expect(
      await screen.findByRole("heading", { name: "New rule" }),
    ).toBeInTheDocument();
  });

  it("opens a new account from the keyboard on the accounts screen", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    expect(
      await screen.findByRole("heading", { name: "Accounts" }),
    ).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "n" });

    expect(
      await screen.findByRole("heading", { name: "New account" }),
    ).toBeInTheDocument();
  });

  it("supports mark-read-and-archive from the TUI manifest action set", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "x" });
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(
      document.querySelector('input[placeholder="Search commands..."]'),
    ).not.toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: /Mark Read and Archive/i }),
    );

    await waitFor(() => {
      expect(
        screen.getByText("Marking 1 message read and archiving"),
      ).toBeInTheDocument();
    });
  });

  it("generates a bug report from diagnostics", async () => {
    installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));
    expect(
      await screen.findByRole("heading", { name: "Diagnostics" }),
    ).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "Generate bug report" }),
    );

    expect(
      await screen.findByRole("heading", { name: "Bug report" }),
    ).toBeInTheDocument();
    expect(await screen.findByText(/bug report body/)).toBeInTheDocument();
  });

  it("opens logs, config, and diagnostics details from the TUI command surface", async () => {
    const desktopApi = installDesktopApi();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await act(async () => {
      await flushAsyncWork();
    });
    fireEvent.click(screen.getByRole("button", { name: /Open Logs/i }));

    await waitFor(() => {
      expect(desktopApi.openLocalPath).toHaveBeenCalledWith("/tmp/mxr.log");
    });

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await act(async () => {
      await flushAsyncWork();
    });
    fireEvent.click(screen.getByRole("button", { name: /Edit Config/i }));

    await waitFor(() => {
      expect(desktopApi.openConfigFile).toHaveBeenCalled();
    });

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await act(async () => {
      await flushAsyncWork();
    });
    fireEvent.click(
      screen.getByRole("button", { name: /Open Diagnostics Details/i }),
    );

    expect(
      await screen.findByRole("heading", { name: "Diagnostics details" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(/Log file: \/tmp\/mxr\.log/),
    ).toBeInTheDocument();
  });

  it("triggers daemon sync via command palette", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    // Open command palette and run sync
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "Sync now" },
    });
    fireEvent.keyDown(window, { key: "Enter" });

    await waitFor(() => {
      expect(findRequest("/sync", "POST")).toBeDefined();
    });
  });

  it("triggers daemon sync from the header action", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));

    await waitFor(() => {
      expect(findRequest("/sync", "POST")).toBeDefined();
    });
    expect(screen.getByText("Syncing with server")).toBeInTheDocument();
  });

  it("toggles remote content via command palette", async () => {
    installDesktopApi();
    installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    // Verify initial state
    expect(document.body.getAttribute("data-remote-content")).toBe("false");

    // Open command palette and toggle remote content
    window.dispatchEvent(new CustomEvent("mxr:command-palette"));
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "Toggle remote content" },
    });
    fireEvent.keyDown(window, { key: "Enter" });

    await waitFor(() => {
      expect(document.body.getAttribute("data-remote-content")).toBe("true");
    });
  });

  it("creates a saved search via command palette", async () => {
    installDesktopApi();
    const { requests } = installFetchMocks();

    renderApp();

    await findActiveLens("Inbox");

    // Open command palette and run save search
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Search commands..."),
      ).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "Save current search" },
    });
    fireEvent.keyDown(window, { key: "Enter" });

    // The saved search dialog should open
    await waitFor(() => {
      expect(screen.getByText("Save search")).toBeInTheDocument();
    });

    // Fill in the name and submit
    const nameInput = screen.getByPlaceholderText("My search");
    fireEvent.change(nameInput, { target: { value: "deploy alerts" } });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const createRequest = requests().find(
        (r) => r.path === "/saved-searches/create",
      );
      expect(createRequest).toBeDefined();
      const body = JSON.parse(createRequest!.body!);
      expect(body.name).toBe("deploy alerts");
    });
  });

  it("shows context menu on right-click with mail actions", async () => {
    installDesktopApi();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    const mailRows = screen.getAllByTestId("mail-row");
    fireEvent.contextMenu(mailRows[0]);

    await waitFor(() => {
      expect(screen.getByText("Archive")).toBeInTheDocument();
      expect(screen.getByText("Apply label")).toBeInTheDocument();
      expect(screen.getByText("Snooze")).toBeInTheDocument();
      expect(screen.getByText("Forward")).toBeInTheDocument();
      expect(screen.getByText("Trash")).toBeInTheDocument();
      expect(screen.getByText("Star")).toBeInTheDocument();
      expect(screen.getByText("Open in browser")).toBeInTheDocument();
    });
  });

  it("executes archive from context menu", async () => {
    installDesktopApi();
    const { requests } = installFetchMocks();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    const mailRows = screen.getAllByTestId("mail-row");
    fireEvent.contextMenu(mailRows[0]);

    await waitFor(() => {
      expect(screen.getByText("Archive")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Archive"));

    await waitFor(() => {
      const archiveRequest = requests().find(
        (r) => r.path === "/mutations/archive",
      );
      expect(archiveRequest).toBeDefined();
    });
  });

  it("supports keyboard navigation in the context menu", async () => {
    installDesktopApi();
    const { requests } = installFetchMocks();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    const mailRows = screen.getAllByTestId("mail-row");
    fireEvent.contextMenu(mailRows[0]);

    await waitFor(() => {
      expect(screen.getByText("Archive")).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "ArrowDown" });
    fireEvent.keyDown(window, { key: "Enter" });

    await waitFor(() => {
      const starRequest = requests().find((r) => r.path === "/mutations/star");
      expect(starRequest).toBeDefined();
    });
  });

  it("opens move-to-label from the context menu and submits the move", async () => {
    installDesktopApi();
    const { requests } = installFetchMocks();

    renderApp();

    await screen.findByRole("button", { name: "Mailbox" });

    const mailRows = screen.getAllByTestId("mail-row");
    fireEvent.contextMenu(mailRows[0]);

    await waitFor(() => {
      expect(screen.getByText("Move to")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Move to"));

    expect(await screen.findByText("Move message")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Target"), {
      target: { value: "Follow Up" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Move" }));

    await waitFor(() => {
      const moveRequest = requests().find(
        (request) => request.path === "/mutations/move",
      );
      expect(moveRequest).toBeDefined();
      expect(
        parseRequestBody<{ message_ids: string[]; target_label: string }>(
          moveRequest,
        ),
      ).toEqual({
        message_ids: ["msg-1"],
        target_label: "Follow Up",
      });
    });
  });
});
