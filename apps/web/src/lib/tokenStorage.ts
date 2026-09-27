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

export function bootstrapFromHash(
  confirmRemote: (origin: string) => boolean = (origin) =>
    window.confirm(
      `Connect this mxr app to the mail bridge at ${origin}?\n\nOnly continue if you opened this link yourself. The app will send your mail requests there.`,
    ),
): void {
  if (typeof window === "undefined") return;
  const hash = window.location.hash;
  if (!hash) return;
  const params = new URLSearchParams(hash.startsWith("#") ? hash.slice(1) : hash);
  const token = params.get("token");
  const remote = params.get("remote");
  const remoteOrigin = remote ? parseOrigin(remote) : null;
  const switchesOrigin =
    remoteOrigin !== null &&
    remoteOrigin !== window.location.origin &&
    remoteOrigin !== safeStorage()?.getItem(REMOTE_URL_KEY);

  // Any page can link here with #remote=…; pointing the app at another
  // bridge takes the user's explicit consent, and the local token never
  // travels to it (a remote link brings its own token, or none).
  if (switchesOrigin && remoteOrigin && confirmRemote(remoteOrigin)) {
    safeStorage()?.setItem(REMOTE_URL_KEY, remoteOrigin);
    if (token) setToken(token);
    else clearToken();
  } else if (!switchesOrigin && token) {
    setToken(token);
  }
  if (token || remote) {
    // scrub the hash so the token isn't shoulder-surfed or copied into bookmarks
    const cleaned = window.location.pathname + window.location.search;
    window.history.replaceState({}, document.title, cleaned);
  }
}

function parseOrigin(remote: string): string | null {
  try {
    // remote may arrive URL-encoded or as a bare host
    const normalized = remote.startsWith("http") ? remote : `https://${remote}`;
    return new URL(normalized).origin;
  } catch {
    return null;
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

const tokenListeners = new Set<(token: string) => void>();

/**
 * Hear about a newly stored token. The event socket starts before the
 * loopback handshake finishes on a first visit, so it listens here to
 * connect as soon as the token lands instead of waiting for a refocus.
 */
export function onTokenChange(listener: (token: string) => void): () => void {
  tokenListeners.add(listener);
  return () => tokenListeners.delete(listener);
}

export function setToken(token: string): void {
  const storage = safeStorage();
  if (!storage) return;
  const previous = storage.getItem(STORAGE_KEY);
  storage.setItem(STORAGE_KEY, token);
  if (previous !== token) for (const listener of tokenListeners) listener(token);
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
