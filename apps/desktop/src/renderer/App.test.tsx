import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BridgeState } from "../shared/types";
import App from "./App";
import { configureDesktopMockServer, getRecordedDesktopRequests } from "./test/desktopMockServer";

const readyBridge = {
  kind: "ready" as const,
  baseUrl: "http://127.0.0.1:4010",
  authToken: "test-token",
  binaryPath: "/usr/local/bin/mxr",
  usingBundled: true,
  daemonVersion: "0.4.4",
  protocolVersion: 1,
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
  detail: "mxr Desktop needs a compatible version of mxr before it can connect.",
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
  const api = {
    getBridgeState: vi.fn().mockResolvedValue(bridgeState),
    retryBridge: vi.fn(),
    useBundledMxr: vi.fn(),
    setExternalBinaryPath: vi.fn(),
    openDraftInEditor: vi.fn().mockResolvedValue({ ok: true }),
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

function installFetchMocks(options?: { delayReadMutation?: Promise<void> }) {
  configureDesktopMockServer(options);
  return {
    requests: getRecordedDesktopRequests,
  };
}

function readMutationCalls() {
  return getRecordedDesktopRequests().filter((request) => request.path === "/mutations/read");
}

function findRequest(path: string, method = "GET") {
  return getRecordedDesktopRequests().find(
    (request) => request.path === path && request.method === method,
  );
}

function findRequestMatching(
  predicate: (request: ReturnType<typeof getRecordedDesktopRequests>[number]) => boolean,
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

    render(<App />);

    expect(
      await screen.findByText("mxr Desktop needs a compatible version of mxr"),
    ).toBeInTheDocument();
    expect(screen.getByText("Homebrew: brew upgrade mxr")).toBeInTheDocument();
    expect(screen.getByText("Use bundled mxr")).toBeInTheDocument();
  });

  it("renders the dark workbench shell and switches screens", async () => {
    installDesktopApi();

    render(<App />);

    await screen.findByRole("button", { name: "Mailbox" });
    expect(screen.getByRole("button", { name: "Search" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rules" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accounts" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Diagnostics" })).toBeInTheDocument();
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

  it("opens the selected thread with the keyboard and closes back to two-pane", async () => {
    installDesktopApi();

    render(<App />);

    await screen.findByRole("button", { name: "Mailbox" });

    fireEvent.keyDown(window, { key: "Enter" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(screen.getAllByRole("button", { name: "Archive" }).length).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "Escape" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(screen.queryAllByRole("button", { name: "Archive" })).toHaveLength(0);
  });

  it("wires the live event stream and refreshes the mailbox on sync completion", async () => {
    installDesktopApi();

    render(<App />);

    await findActiveLens("Inbox");
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(
      getRecordedDesktopRequests().filter((request) => request.path === "/mailbox"),
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
        getRecordedDesktopRequests().filter((request) => request.path === "/mailbox"),
      ).toHaveLength(2);
    });
  });

  it("switches mailbox lenses from the sidebar", async () => {
    installDesktopApi();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: /All Mail/i }));

    await waitFor(() => {
      expect(getActiveLens("All Mail")).toBeInTheDocument();
    });
    expect(screen.getAllByText("Review requested").length).toBeGreaterThan(0);
  });

  it("shows the mac command hint and opens the command palette from the app shortcut event", async () => {
    setNavigatorPlatform("MacIntel");
    installDesktopApi();

    render(<App />);

    await screen.findByRole("button", { name: "Mailbox" });
    expect(document.body.textContent).toContain("⌘P");

    fireEvent.keyDown(window, { key: "2" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(document.body.textContent).toContain("Search");

    window.dispatchEvent(new CustomEvent("mxr:command-palette"));
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands...")).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "g" });
    fireEvent.keyDown(window, { key: "i" });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(screen.queryByRole("tab", { name: "Threads" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mailbox" })).toBeInTheDocument();
  });

  it("selects the first filtered command and runs it on enter", async () => {
    installDesktopApi();

    render(<App />);

    await screen.findByRole("button", { name: "Mailbox" });

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands...")).toBeInTheDocument();
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

    render(<App />);

    await screen.findByRole("button", { name: "Mailbox" });

    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands...")).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(document.querySelector('[aria-selected="true"]')).not.toBeNull();
    });
    const firstSelectedText = document.querySelector('[aria-selected="true"]')?.textContent;

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

    render(<App />);

    await findActiveLens("Inbox");
    scrollSpy.mockClear();

    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(scrollSpy).toHaveBeenCalledWith({ block: "nearest" });
    });
  });

  it("extends a continuous selection in visual mode while moving with j", async () => {
    installDesktopApi();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "V" });
    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(screen.getByText("Vercel").closest("button")?.className).toContain("bg-success");
    });
    expect(screen.getByText("Stripe").closest("button")?.className).toContain("bg-panel-elevated");
  });

  it("keeps the search selection in view while navigating with j", async () => {
    installDesktopApi();
    const scrollSpy = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      value: scrollSpy,
      configurable: true,
    });

    render(<App />);

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

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "h" });
    fireEvent.keyDown(window, { key: "j" });

    await waitFor(() => {
      expect(getActiveLens("All Mail")).toBeInTheDocument();
    });
  });

  it("dispatches manifest-driven star mutations from the mail list", async () => {
    installDesktopApi();
    installFetchMocks();

    render(<App />);

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

    render(<App />);

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
      expect(screen.queryByText("Marking 1 message read")).not.toBeInTheDocument();
    });
  });

  it("delays preview mark-read until the reader settles on one message for five seconds", async () => {
    vi.useFakeTimers();
    installDesktopApi();
    installFetchMocks();

    try {
      render(<App />);
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
        parseRequestBody<{ message_ids: string[]; read: boolean }>(readMutationCalls()[0]),
      ).toEqual({
        message_ids: ["msg-2"],
        read: true,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens compose, launches the editor, and sends the draft", async () => {
    const desktopApi = installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "c" });

    expect(await screen.findByRole("heading", { name: "New message" })).toBeInTheDocument();

    // Step 1: recipient picker shown with contact list
    expect(screen.getByPlaceholderText("Type a name or email...")).toBeInTheDocument();
  });

  it("opens a reply shell for the selected message", async () => {
    const desktopApi = installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "r" });

    expect(await screen.findByRole("heading", { name: "Reply" })).toBeInTheDocument();

    const composeCall = findRequest("/compose/session", "POST");
    expect(composeCall).toBeDefined();
    expect(parseRequestBody<{ kind: string; message_id: string }>(composeCall)).toMatchObject({
      kind: "reply",
      message_id: "msg-1",
    });
    // Step 1: recipients shown with Write body button
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
    });
  });

  it("applies labels and moves the selected message", async () => {
    installDesktopApi();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Label" }));
    await act(async () => {
      await flushAsyncWork();
    });
    expect(document.body.textContent).toContain("Apply label");
    fireEvent.click(screen.getByRole("checkbox", { name: "Follow Up" }));
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
    expect(screen.getByRole("option", { name: "Follow Up" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Move" })).toBeInTheDocument();
  });

  it("supports richer search controls plus snooze and unsubscribe flows", async () => {
    installDesktopApi();
    installFetchMocks();

    render(<App />);

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
    fireEvent.change(screen.getByPlaceholderText("Search subjects, senders, snippets"), {
      target: { value: "deploy" },
    });

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
    expect(await screen.findByRole("heading", { name: "Deploy complete" })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "Z" });

    expect(await screen.findByRole("heading", { name: "Snooze message" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Snooze" }));

    await act(async () => {
      await flushAsyncWork();
    });
    expect(findRequest("/actions/snooze", "POST")).toBeDefined();

    fireEvent.keyDown(window, { key: "D" });

    expect(await screen.findByRole("heading", { name: "Unsubscribe" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Unsubscribe" }));

    await act(async () => {
      await flushAsyncWork();
    });
    expect(findRequest("/actions/unsubscribe", "POST")).toBeDefined();
  });

  it("leaves the search input and returns to keyboard navigation on enter", async () => {
    installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    const input = await screen.findByPlaceholderText("Search subjects, senders, snippets");

    fireEvent.change(input, {
      target: { value: "deploy" },
    });
    await screen.findByRole("heading", { name: "Deploy complete" });

    expect(document.activeElement).toBe(input);

    fireEvent.keyDown(input, { key: "Enter" });
    expect(document.activeElement).not.toBe(input);

    fireEvent.keyDown(window, { key: "j" });
    expect((input as HTMLInputElement).value).toBe("deploy");
  });

  it("opens browser links, exports threads, and opens attachment/link dialogs", async () => {
    const desktopApi = installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "Enter" });
    expect((await screen.findAllByRole("button", { name: "Archive" })).length).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "O" });

    await waitFor(() => {
      expect(desktopApi.openExternalUrl).toHaveBeenCalledWith(
        "https://mail.google.com/mail/u/0/#inbox/gmail-msg-1",
      );
    });

    fireEvent.keyDown(window, { key: "A" });
    expect(await screen.findByRole("heading", { name: "Attachments" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => {
      expect(findRequest("/attachments/open", "POST")).toBeDefined();
    });

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "E" });

    expect(await screen.findByRole("heading", { name: "Thread export" })).toBeInTheDocument();
    expect(await screen.findByText(/Export body/)).toBeInTheDocument();
  });

  it("loads rules and accounts workspaces with real actions", async () => {
    installDesktopApi();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Rules" }));
    expect(await screen.findByRole("heading", { name: "Rules" })).toBeInTheDocument();
    expect(await screen.findByText("Archive receipts")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "History" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dry run" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(await screen.findByRole("heading", { name: "New rule" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    expect(await screen.findByRole("heading", { name: "Accounts" })).toBeInTheDocument();
    expect((await screen.findAllByText("Personal")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Test" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set default" })).toBeInTheDocument();
  });

  it("supports mark-read-and-archive from the TUI manifest action set", async () => {
    installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.keyDown(window, { key: "x" });
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await act(async () => {
      await flushAsyncWork();
    });
    expect(document.querySelector('input[placeholder="Search commands..."]')).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Mark Read and Archive/i }));

    await waitFor(() => {
      expect(screen.getByText("Marking 1 message read and archiving")).toBeInTheDocument();
    });
  });

  it("generates a bug report from diagnostics", async () => {
    installDesktopApi();

    render(<App />);

    await findActiveLens("Inbox");

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));
    expect(await screen.findByRole("heading", { name: "Diagnostics" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Generate bug report" }));

    expect(await screen.findByRole("heading", { name: "Bug report" })).toBeInTheDocument();
    expect(await screen.findByText(/bug report body/)).toBeInTheDocument();
  });

  it("opens logs, config, and diagnostics details from the TUI command surface", async () => {
    const desktopApi = installDesktopApi();

    render(<App />);

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
    fireEvent.click(screen.getByRole("button", { name: /Open Diagnostics Details/i }));

    expect(await screen.findByRole("heading", { name: "Diagnostics details" })).toBeInTheDocument();
    expect(await screen.findByText(/Log file: \/tmp\/mxr\.log/)).toBeInTheDocument();
  });

  it("triggers daemon sync via command palette", async () => {
    installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    // Open command palette and run sync
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands...")).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("Search commands..."), {
      target: { value: "Sync now" },
    });
    fireEvent.keyDown(window, { key: "Enter" });

    await waitFor(() => {
      expect(findRequest("/sync", "POST")).toBeDefined();
    });
  });

  it("toggles remote content via command palette", async () => {
    installDesktopApi();
    installFetchMocks();

    render(<App />);

    await findActiveLens("Inbox");

    // Verify initial state
    expect(document.body.getAttribute("data-remote-content")).toBe("false");

    // Open command palette and toggle remote content
    window.dispatchEvent(new CustomEvent("mxr:command-palette"));
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands...")).toBeInTheDocument();
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

    render(<App />);

    await findActiveLens("Inbox");

    // Open command palette and run save search
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands...")).toBeInTheDocument();
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
      const createRequest = requests().find((r) => r.path === "/saved-searches/create");
      expect(createRequest).toBeDefined();
      const body = JSON.parse(createRequest!.body!);
      expect(body.name).toBe("deploy alerts");
    });
  });

  it("shows context menu on right-click with mail actions", async () => {
    installDesktopApi();

    render(<App />);

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

    render(<App />);

    await screen.findByRole("button", { name: "Mailbox" });

    const mailRows = screen.getAllByTestId("mail-row");
    fireEvent.contextMenu(mailRows[0]);

    await waitFor(() => {
      expect(screen.getByText("Archive")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Archive"));

    await waitFor(() => {
      const archiveRequest = requests().find((r) => r.path === "/mutations/archive");
      expect(archiveRequest).toBeDefined();
    });
  });
});
