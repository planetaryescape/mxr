/*
 * Bridge bearer-token storage.
 *
 * Bootstrap precedence on first load:
 *   1. URL fragment `#token=...` (remote/manual launch only)
 *   2. localStorage `mxr.bridgeToken`
 *   3. nothing — UI surfaces a paste-token settings panel after first 401
 */

const STORAGE_KEY = "mxr.bridgeToken";
const REMOTE_URL_KEY = "mxr.bridgeUrl";

export function bootstrapFromHash(): void {
  if (typeof window === "undefined") return;
  const hash = window.location.hash;
  if (!hash) return;
  const params = new URLSearchParams(hash.startsWith("#") ? hash.slice(1) : hash);
  const token = params.get("token");
  const remote = params.get("remote");
  if (token) {
    setToken(token);
  }
  if (remote) {
    try {
      // remote may arrive URL-encoded or as a bare host
      const normalized = remote.startsWith("http") ? remote : `https://${remote}`;
      const url = new URL(normalized);
      safeStorage()?.setItem(REMOTE_URL_KEY, url.origin);
    } catch {
      // ignore malformed remote
    }
  }
  if (token || remote) {
    // scrub the hash so the token isn't shoulder-surfed or copied into bookmarks
    const cleaned = window.location.pathname + window.location.search;
    window.history.replaceState({}, document.title, cleaned);
  }
}

function safeStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

export function getToken(): string | undefined {
  const storage = safeStorage();
  if (!storage) return undefined;
  const v = storage.getItem(STORAGE_KEY);
  return v ?? undefined;
}

export function setToken(token: string): void {
  const storage = safeStorage();
  if (!storage) return;
  storage.setItem(STORAGE_KEY, token);
}

export function clearToken(): void {
  const storage = safeStorage();
  if (!storage) return;
  storage.removeItem(STORAGE_KEY);
}

export function getBridgeBaseUrl(): string {
  // In dev, Vite proxies /api → daemon; same-origin works. In prod the SPA is
  // served from the daemon itself so same-origin also works. Remote-host mode
  // sets a different origin via URL fragment on first load.
  if (typeof window === "undefined") return "";
  const storage = safeStorage();
  const remote = storage?.getItem(REMOTE_URL_KEY) ?? null;
  return remote ?? window.location.origin;
}

export function getBridgeWsUrl(): string {
  const base = getBridgeBaseUrl();
  return base.replace(/^http/, "ws");
}
