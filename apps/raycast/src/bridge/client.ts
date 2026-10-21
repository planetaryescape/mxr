import { getPreferenceValues } from "@raycast/api";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

/** The daemon's default bridge port, used when it has not written its port file. */
const DEFAULT_PORT = 42829;
/** A first build of a mode view takes about a second on a large mailbox. */
const TIMEOUT_MS = 20_000;

interface Preferences {
  dataDirectory?: string;
}

export class BridgeError extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
    this.name = "BridgeError";
  }
}

function dataDirectory(): string {
  const { dataDirectory } = getPreferenceValues<Preferences>();
  return dataDirectory?.trim() || join(homedir(), "Library", "Application Support", "mxr");
}

async function readTrimmed(path: string): Promise<string | undefined> {
  try {
    const text = (await readFile(path, "utf8")).trim();
    return text || undefined;
  } catch {
    return undefined;
  }
}

/** Where the daemon's bridge listens, from the port file it writes on start. */
export async function bridgeBase(): Promise<string> {
  const port = Number(await readTrimmed(join(dataDirectory(), "bridge-port")));
  return `http://127.0.0.1:${Number.isInteger(port) && port > 0 ? port : DEFAULT_PORT}`;
}

/** The web app on the same port, by the name the daemon prints for it. */
export async function webBase(): Promise<string> {
  return (await bridgeBase()).replace("127.0.0.1", "mxr.localhost");
}

let cachedToken: string | undefined;

/**
 * The bridge token: the cached one, else the daemon's token file. After a
 * 401, `rejected` is the token that failed, so a rotated file is reread
 * first and the loopback handshake (which a daemon may turn off) is the
 * fallback, as it is for the web app.
 */
async function token(base: string, rejected?: string): Promise<string> {
  if (cachedToken && cachedToken !== rejected) return cachedToken;
  const fromFile = await readTrimmed(join(dataDirectory(), "bridge-token"));
  if (fromFile && fromFile !== rejected) return (cachedToken = fromFile);
  const response = await send(`${base}/api/v1/auth/local-token`, { method: "GET" });
  if (!response.ok) {
    cachedToken = undefined;
    throw new BridgeError(
      "mxr rejected the bridge token and did not hand over a new one. Check the data directory in this extension's preferences.",
      response.status,
    );
  }
  const body = (await response.json()) as { token?: string };
  if (!body.token) throw new BridgeError("mxr returned an empty bridge token.");
  return (cachedToken = body.token);
}

async function send(url: string, init: RequestInit): Promise<Response> {
  try {
    return await fetch(url, { ...init, signal: AbortSignal.timeout(TIMEOUT_MS) });
  } catch (error) {
    if (error instanceof Error && error.name === "TimeoutError") {
      throw new BridgeError("mxr took too long to answer. Is a sync or reindex running?");
    }
    throw new BridgeError("mxr is not running. Start it with `mxr daemon` or open the mxr app.");
  }
}

export interface RequestOptions {
  method?: "GET" | "POST" | "DELETE";
  body?: unknown;
  headers?: Record<string, string>;
}

/** Call the daemon's HTTP bridge and return its JSON body. */
export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const base = await bridgeBase();
  const attempt = async (bearer: string) =>
    send(`${base}${path}`, {
      method: options.method ?? (options.body === undefined ? "GET" : "POST"),
      headers: {
        Authorization: `Bearer ${bearer}`,
        ...(options.body === undefined ? {} : { "Content-Type": "application/json" }),
        ...options.headers,
      },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    });
  const first = await token(base);
  let response = await attempt(first);
  if (response.status === 401) response = await attempt(await token(base, first));
  if (!response.ok) throw new BridgeError(await failure(response), response.status);
  return (await response.json()) as T;
}

async function failure(response: Response): Promise<string> {
  const text = await response.text();
  try {
    const body = JSON.parse(text) as { error?: string; message?: string };
    return body.error ?? body.message ?? `mxr answered ${response.status}`;
  } catch {
    return text.trim() || `mxr answered ${response.status}`;
  }
}
