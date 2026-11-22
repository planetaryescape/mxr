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

function respond(
  status: number,
  statusText: string,
  body: string,
  contentType?: string,
  extraHeaders?: Record<string, string>,
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(body, {
          status,
          statusText,
          headers: {
            ...(contentType ? { "content-type": contentType } : {}),
            ...extraHeaders,
          },
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
});

describe("telling a stopped daemon from a failed request", () => {
  test("an offline cached success remains readable without claiming the daemon is healthy", async () => {
    const heard: boolean[] = [];
    const off = onDaemonReachability((reachable) => heard.push(reachable));
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Response.json({ saved: true }, { headers: { "x-mxr-offline": "1" } })),
    );
    try {
      await expect(apiFetch("/api/v1/mail/threads/saved")).resolves.toEqual({ saved: true });
      expect(heard).toEqual([false]);
    } finally {
      off();
    }
  });

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

  test("a marked tailnet denial is an outage, not a permission verdict", async () => {
    const heard: boolean[] = [];
    const off = onDaemonReachability((reachable) => heard.push(reachable));
    respond(
      403,
      "Forbidden",
      JSON.stringify({ error: "mxr is available over Tailscale only." }),
      "application/json",
      { "x-mxr-tailnet-required": "1" },
    );
    expect(await failure()).toBeInstanceOf(DaemonUnavailableError);
    expect(heard).toEqual([false]);
    off();
  });
});
