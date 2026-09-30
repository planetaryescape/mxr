/*
 * Typed HTTP client over openapi-fetch with auth middleware that pulls
 * the bearer token from localStorage. Surfaces a `client` for queries
 * and an `apiFetch` raw helper for endpoints that aren't yet typed by
 * the generated OpenAPI surface.
 *
 * Same-machine bootstrap: when no token is present, or when a request
 * returns 401, we transparently retry once via `tryLocalHandshake()`
 * (see `@/lib/localHandshake`). On loopback with `[bridge].auto_local_token`
 * enabled (default), this means the SPA self-authenticates without ever
 * showing the paste-token panel.
 */

import createClient, { type Middleware } from "openapi-fetch";

import type { paths } from "./generated";
import { tryLocalHandshake } from "@/lib/localHandshake";
import { clearToken, getBridgeBaseUrl, getToken } from "@/lib/tokenStorage";

export class UnauthorizedError extends Error {
  constructor() {
    super("Unauthorized");
    this.name = "UnauthorizedError";
  }
}

/**
 * A non-2xx bridge response. The bridge answers failures with a JSON body
 * carrying a human `error` string, sometimes a machine-readable `code`, and
 * sometimes extra fields a caller can act on. Unwrapping it here means callers
 * render the daemon's own sentence instead of `502 Bad Gateway: {"error":…}`,
 * and can branch on `code` rather than matching message text.
 */
export class BridgeRequestError extends Error {
  readonly status: number;
  readonly code?: string;
  readonly details: Record<string, unknown>;

  constructor(status: number, message: string, code?: string, details?: Record<string, unknown>) {
    super(message);
    this.name = "BridgeRequestError";
    this.status = status;
    this.code = code;
    this.details = details ?? {};
  }
}

/**
 * No answer from mxr's daemon: the bridge is down with it, or up but unable
 * to reach its socket, or a proxy in front of it gave up. The request never
 * reached the daemon, so nothing it asked for happened.
 */
export class DaemonUnavailableError extends Error {
  constructor(options?: { cause?: unknown }) {
    super("Couldn't reach mxr's daemon. It may be stopped or restarting.", options);
    this.name = "DaemonUnavailableError";
  }
}

type ReachabilityListener = (reachable: boolean) => void;
const reachabilityListeners = new Set<ReachabilityListener>();

/**
 * Hear whether each bridge request reached the daemon. `lib/daemonAvailability`
 * turns this into the app's one "is the daemon up" state; it lives here as a
 * listener so the client doesn't import the state it feeds.
 */
export function onDaemonReachability(listener: ReachabilityListener): () => void {
  reachabilityListeners.add(listener);
  return () => reachabilityListeners.delete(listener);
}

function reportReachability(reachable: boolean): void {
  for (const listener of reachabilityListeners) listener(reachable);
}

async function sendToBridge(path: string, init: RequestInit): Promise<Response> {
  let res: Response;
  try {
    res = await fetch(`${getBridgeBaseUrl()}${path}`, init);
  } catch (error) {
    if (init.signal?.aborted) throw error;
    // fetch only rejects when no response came back at all.
    reportReachability(false);
    throw new DaemonUnavailableError({ cause: error });
  }
  // A 5xx is judged by its body in `failureFrom`.
  if (res.status < 500) reportReachability(true);
  return res;
}

/**
 * `connect` is the bridge unable to reach the daemon's socket. An empty 5xx
 * never comes from the bridge, which always says what failed; it is the dev
 * proxy (or another in front of the bridge) finding nothing to talk to.
 */
async function failureFrom(res: Response): Promise<Error> {
  const text = await res.text().catch(() => "");
  const failure = bridgeRequestError(res.status, res.statusText, text);
  if (res.status < 500) return failure;
  const unreachable = failure.code === "connect" || text.trim() === "";
  reportReachability(!unreachable);
  return unreachable ? new DaemonUnavailableError({ cause: failure }) : failure;
}

function bridgeRequestError(status: number, statusText: string, body: string): BridgeRequestError {
  const fallback = `${status} ${statusText}${body ? `: ${body}` : ""}`;
  const details = jsonObject(body);
  if (!details) return new BridgeRequestError(status, fallback);
  const message = typeof details.error === "string" ? details.error : "";
  const code = typeof details.code === "string" ? details.code : undefined;
  return new BridgeRequestError(status, message || fallback, code, details);
}

function jsonObject(body: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(body);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

const authMiddleware: Middleware = {
  async onRequest({ request }) {
    let token = getToken();
    if (!token) {
      token = await tryLocalHandshake();
    }
    if (token) {
      request.headers.set("Authorization", `Bearer ${token}`);
    }
    return request;
  },
  async onResponse({ response, request }) {
    if (response.status === 401) {
      const recovered = await tryLocalHandshake();
      if (recovered) {
        const retry = new Request(request, {});
        retry.headers.set("Authorization", `Bearer ${recovered}`);
        const second = await fetch(retry);
        if (second.status !== 401) return second;
      }
      throw new UnauthorizedError();
    }
    return response;
  },
};

const client = createClient<paths>({
  baseUrl: getBridgeBaseUrl(),
});

client.use(authMiddleware);

export const api = client;

export interface RawFetchOpts {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  signal?: AbortSignal;
}

export async function apiFetch<T>(path: string, opts: RawFetchOpts = {}): Promise<T> {
  let token = getToken();
  if (!token) token = await tryLocalHandshake();
  const headers = new Headers({ "content-type": "application/json" });
  if (token) headers.set("authorization", `Bearer ${token}`);
  const init: RequestInit = {
    method: opts.method ?? "GET",
    headers,
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    signal: opts.signal,
  };
  let res = await sendToBridge(path, init);
  if (res.status === 401) {
    const recovered = await tryLocalHandshake();
    if (recovered) {
      headers.set("authorization", `Bearer ${recovered}`);
      res = await sendToBridge(path, init);
    }
  }
  if (res.status === 401) throw new UnauthorizedError();
  if (!res.ok) throw await failureFrom(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Like `apiFetch`, for endpoints that return bytes (inline images). */
export async function apiFetchBlob(
  path: string,
  opts: { signal?: AbortSignal } = {},
): Promise<Blob> {
  let token = getToken();
  if (!token) token = await tryLocalHandshake();
  const headers = new Headers();
  if (token) headers.set("authorization", `Bearer ${token}`);
  const res = await sendToBridge(path, { headers, signal: opts.signal });
  if (res.status === 401) throw new UnauthorizedError();
  if (!res.ok) throw await failureFrom(res);
  return res.blob();
}

export function logoutAndReload(): void {
  clearToken();
  if (typeof window !== "undefined") window.location.reload();
}
