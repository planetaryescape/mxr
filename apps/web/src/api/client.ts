import {
  assertTarget,
  assertWorkspaceBinding,
  observeWorkspace,
  parseWorkspace,
  selectedWorkspace,
  workspaceGeneration,
  WorkspaceConflictError,
  type WorkspaceBinding,
} from "@/lib/workspaceOwner";
import createClient, { type Middleware } from "openapi-fetch";

import type { paths } from "./generated";
import { tryLocalHandshake } from "@/lib/localHandshake";
import { clearToken, getBridgeBaseUrl, getToken } from "@/lib/tokenStorage";
import { OFFLINE_HEADER } from "@/pwa/offlineReads";

export class UnauthorizedError extends Error {
  constructor() {
    super("Unauthorized");
    this.name = "UnauthorizedError";
  }
}

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

export class DaemonUnavailableError extends Error {
  constructor(options?: { cause?: unknown }) {
    super("Couldn't reach mxr's daemon. It may be stopped or restarting.", options);
    this.name = "DaemonUnavailableError";
  }
}

type ReachabilityListener = (reachable: boolean) => void;
const reachabilityListeners = new Set<ReachabilityListener>();

export function onDaemonReachability(listener: ReachabilityListener): () => void {
  reachabilityListeners.add(listener);
  return () => reachabilityListeners.delete(listener);
}

function reportReachability(reachable: boolean): void {
  for (const listener of reachabilityListeners) listener(reachable);
}

async function sendToBridge(
  path: string,
  init: RequestInit,
  target = getBridgeBaseUrl(),
  binding?: WorkspaceBinding,
): Promise<Response> {
  let res: Response;
  try {
    assertTarget(target);
    if (binding) assertWorkspaceBinding(binding);
    res = await fetch(`${target}${path}`, init);
  } catch (error) {
    if (init.signal?.aborted || error instanceof WorkspaceConflictError) throw error;
    reportReachability(false);
    throw new DaemonUnavailableError({ cause: error });
  }
  if (
    res.headers.get(OFFLINE_HEADER) === "1" ||
    res.headers.get("x-mxr-tailnet-required") === "1"
  )
    reportReachability(false);
  else if (res.status < 500) reportReachability(true);
  return res;
}

async function failureFrom(res: Response): Promise<Error> {
  const text = await res.text().catch(() => "");
  const failure = bridgeRequestError(res.status, res.statusText, text);
  if (res.status === 403 && res.headers.get("x-mxr-tailnet-required") === "1") {
    return new DaemonUnavailableError({ cause: failure });
  }
  if (res.status < 500) return failure;
  if (res.headers.get(OFFLINE_HEADER) === "1") return failure;
  const isJson = jsonObject(text) !== null;
  const unreachable =
    failure.code === "connect" ||
    text.trim() === "" ||
    (!isJson &&
      (res.status >= 502 || text.includes("ECONNREFUSED") || text.includes("fetch failed")));
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

interface RequestOwnerContext {
  target: string;
  generation: number;
  binding?: WorkspaceBinding;
}
const typedContexts = new WeakMap<Request, RequestOwnerContext>();
function observeResponseOwner(response: Response, context: RequestOwnerContext): void {
  assertTarget(context.target);
  if (context.generation !== workspaceGeneration()) throw new WorkspaceConflictError();
  const header = response.headers.get("x-mxr-workspace");
  if (header) {
    const workspace = parseWorkspace(JSON.parse(header));
    if (workspace) observeWorkspace({ target: context.target, workspace });
  }
  if (context.generation !== workspaceGeneration()) throw new WorkspaceConflictError();
  if (context.binding) assertWorkspaceBinding(context.binding);
}
const authMiddleware: Middleware = {
  async onRequest({ request }) {
    const context: RequestOwnerContext = {
      target: getBridgeBaseUrl(),
      generation: workspaceGeneration(),
      binding: request.method === "GET" ? undefined : selectedWorkspace(),
    };
    if (new URL(request.url).origin !== new URL(context.target).origin)
      throw new WorkspaceConflictError();
    let token = getToken();
    if (!token) token = await tryLocalHandshake();
    assertTarget(context.target);
    if (context.binding) assertWorkspaceBinding(context.binding);
    if (token) request.headers.set("Authorization", `Bearer ${token}`);
    if (context.binding)
      request.headers.set("x-mxr-workspace", JSON.stringify(context.binding.workspace));
    typedContexts.set(request, context);
    return request;
  },
  async onResponse({ response, request }) {
    const context = typedContexts.get(request);
    if (!context) throw new Error("Typed request lost its captured workspace context");
    observeResponseOwner(response, context);
    if (response.status === 401) {
      const recovered = await tryLocalHandshake();
      if (recovered) {
        assertTarget(context.target);
        if (context.binding) assertWorkspaceBinding(context.binding);
        const retry = new Request(request, {});
        retry.headers.set("Authorization", `Bearer ${recovered}`);
        const second = await fetch(retry);
        observeResponseOwner(second, context);
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
  binding?: WorkspaceBinding;
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  signal?: AbortSignal;
}

export async function apiFetch<T>(path: string, opts: RawFetchOpts = {}): Promise<T> {
  const target = opts.binding?.target ?? getBridgeBaseUrl();
  const commandBinding =
    opts.binding ?? (opts.method && opts.method !== "GET" ? selectedWorkspace() : undefined);
  const startedGeneration = workspaceGeneration();
  assertTarget(target);
  if (commandBinding) assertWorkspaceBinding(commandBinding);
  let token = getToken();
  if (!token) token = await tryLocalHandshake();
  assertTarget(target);
  if (commandBinding) assertWorkspaceBinding(commandBinding);
  const headers = new Headers({ "content-type": "application/json" });
  if (token) headers.set("authorization", `Bearer ${token}`);
  if (commandBinding) headers.set("x-mxr-workspace", JSON.stringify(commandBinding.workspace));
  const init: RequestInit = {
    method: opts.method ?? "GET",
    headers,
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    signal: opts.signal,
  };
  let res = await sendToBridge(path, init, target, commandBinding);
  if (res.status === 401) {
    const recovered = await tryLocalHandshake();
    if (recovered) {
      headers.set("authorization", `Bearer ${recovered}`);
      res = await sendToBridge(path, init, target, commandBinding);
    }
  }
  observeResponseOwner(res, { target, generation: startedGeneration, binding: commandBinding });
  if (res.status === 401) throw new UnauthorizedError();
  if (!res.ok) throw await failureFrom(res);
  if (res.status === 204) return undefined as T;
  const result = (await res.json()) as T;
  observeResponseOwner(res, { target, generation: startedGeneration, binding: commandBinding });
  return result;
}

export async function apiFetchBlob(
  path: string,
  opts: { signal?: AbortSignal } = {},
): Promise<Blob> {
  const context = { target: getBridgeBaseUrl(), generation: workspaceGeneration() };
  let token = getToken();
  if (!token) token = await tryLocalHandshake();
  assertTarget(context.target);
  const headers = new Headers();
  if (token) headers.set("authorization", `Bearer ${token}`);
  const res = await sendToBridge(path, { headers, signal: opts.signal }, context.target);
  observeResponseOwner(res, context);
  if (res.status === 401) throw new UnauthorizedError();
  if (!res.ok) throw await failureFrom(res);
  const blob = await res.blob();
  observeResponseOwner(res, context);
  return blob;
}

export function logoutAndReload(): void {
  clearToken();
  if (typeof window !== "undefined") window.location.reload();
}

export async function captureWorkspaceBinding(): Promise<WorkspaceBinding> {
  const target = getBridgeBaseUrl();
  const status = await apiFetch<{ workspace_owner_supported?: boolean }>("/api/v1/admin/status");
  if (status.workspace_owner_supported !== true)
    throw new Error(
      "Daemon does not support durable workspace ownership; upgrade before opening private compose work",
    );
  const result = await apiFetch<{ workspace: unknown }>(
    "/api/v1/mail/archive-operations/workspace",
  );
  assertTarget(target);
  const workspace = parseWorkspace(result.workspace);
  if (!workspace)
    throw new Error("Daemon does not support durable workspace identity; upgrade before editing");
  const binding = { target, workspace };
  observeWorkspace(binding);
  return binding;
}
