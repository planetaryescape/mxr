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
    throw new Error(`${res.status} ${res.statusText}${text ? `: ${text}` : ""}`);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export function logoutAndReload(): void {
  clearToken();
  if (typeof window !== "undefined") window.location.reload();
}
