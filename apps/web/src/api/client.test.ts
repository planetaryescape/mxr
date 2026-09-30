import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import {
  apiFetch,
  BridgeRequestError,
  DaemonUnavailableError,
  onDaemonReachability,
} from "./client";

vi.mock("@/lib/localHandshake", () => ({
  tryLocalHandshake: vi.fn<() => Promise<string>>(async () => "test-token"),
}));

vi.mock("@/lib/tokenStorage", () => ({
  clearToken: vi.fn<() => void>(),
  getBridgeBaseUrl: () => "http://127.0.0.1:7777",
  getToken: () => "test-token",
}));

function respond(status: number, statusText: string, body: string, contentType?: string) {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(body, {
          status,
          statusText,
          headers: contentType ? { "content-type": contentType } : undefined,
        }),
    ),
  );
}

describe("apiFetch failure reporting", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  test("surfaces the bridge's own sentence, not the raw JSON envelope", async () => {
    respond(
      502,
      "Bad Gateway",
      JSON.stringify({ error: "failed to connect to mxr daemon at /tmp/mxr.sock" }),
      "application/json",
    );

    const error = await apiFetch("/api/v1/mail/status").catch((thrown: unknown) => thrown);

    expect(error).toBeInstanceOf(BridgeRequestError);
    const bridgeError = error as BridgeRequestError;
    expect(bridgeError.message).toBe("failed to connect to mxr daemon at /tmp/mxr.sock");
    expect(bridgeError.message).not.toContain("Bad Gateway");
    expect(bridgeError.message).not.toContain("{");
    expect(bridgeError.status).toBe(502);
  });

  test("carries the machine-readable code and payload so callers can branch on it", async () => {
    respond(
      409,
      "Conflict",
      JSON.stringify({
        error: "this draft has an HTML body",
        code: "html_draft_not_editable",
        previewHtml: "<p>hi</p>",
      }),
      "application/json",
    );

    const error = (await apiFetch("/api/v1/mail/compose/session/restore", {
      method: "POST",
    }).catch((thrown: unknown) => thrown)) as BridgeRequestError;

    expect(error.code).toBe("html_draft_not_editable");
    expect(error.details.previewHtml).toBe("<p>hi</p>");
  });

  test("falls back to status plus body when the failure is not JSON", async () => {
    respond(500, "Internal Server Error", "upstream exploded");

    const error = (await apiFetch("/api/v1/mail/status").catch(
      (thrown: unknown) => thrown,
    )) as BridgeRequestError;

    expect(error.message).toBe("500 Internal Server Error: upstream exploded");
    expect(error.code).toBeUndefined();
  });
});

describe("telling a stopped daemon from a failed request", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  async function failure(): Promise<unknown> {
    return apiFetch("/api/v1/mail/desk").catch((thrown: unknown) => thrown);
  }

  test("no response at all, the bridge unable to reach the socket, or an empty 5xx from a proxy", async () => {
    const heard: boolean[] = [];
    const off = onDaemonReachability((reachable) => heard.push(reachable));
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Failed to fetch");
      }),
    );
    expect(await failure()).toBeInstanceOf(DaemonUnavailableError);

    respond(503, "Service Unavailable", JSON.stringify({ error: "no socket", code: "connect" }));
    expect(await failure()).toBeInstanceOf(DaemonUnavailableError);

    respond(500, "Internal Server Error", "");
    const proxied = await failure();
    expect(proxied).toBeInstanceOf(DaemonUnavailableError);
    expect((proxied as Error).message).toBe(
      "Couldn't reach mxr's daemon. It may be stopped or restarting.",
    );
    expect(heard).toEqual([false, false, false]);
    off();
  });

  test("an answer from the daemon, even a failing one, means it is there", async () => {
    const heard: boolean[] = [];
    const off = onDaemonReachability((reachable) => heard.push(reachable));
    respond(504, "Gateway Timeout", JSON.stringify({ error: "slow", code: "timeout" }));
    expect(await failure()).toBeInstanceOf(BridgeRequestError);
    respond(404, "Not Found", JSON.stringify({ error: "no thread", code: "not_found" }));
    expect(await failure()).toBeInstanceOf(BridgeRequestError);
    respond(200, "OK", "{}", "application/json");
    await apiFetch("/api/v1/mail/desk");
    expect(heard).toEqual([true, true, true]);
    off();
  });

  test("an aborted request is the caller's doing, not an outage", async () => {
    const heard: boolean[] = [];
    const off = onDaemonReachability((reachable) => heard.push(reachable));
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new DOMException("aborted", "AbortError");
      }),
    );
    const controller = new AbortController();
    controller.abort();
    const error = await apiFetch("/api/v1/mail/desk", { signal: controller.signal }).catch(
      (thrown: unknown) => thrown,
    );
    expect(error).not.toBeInstanceOf(DaemonUnavailableError);
    expect(heard).toEqual([]);
    off();
  });
});
