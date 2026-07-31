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
  let res = await fetch(`${getBridgeBaseUrl()}${path}`, init);
  if (res.status === 401) {
    const recovered = await tryLocalHandshake();
    if (recovered) {
      headers.set("authorization", `Bearer ${recovered}`);
      res = await fetch(`${getBridgeBaseUrl()}${path}`, init);
    }
  }
  if (res.status === 401) throw new UnauthorizedError();
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw bridgeRequestError(res.status, res.statusText, text);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export function logoutAndReload(): void {
  clearToken();
  if (typeof window !== "undefined") window.location.reload();
}
