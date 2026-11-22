import { clearOfflineMail } from "@/pwa/offlineReads";

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

  if (switchesOrigin && remoteOrigin && confirmRemote(remoteOrigin)) {
    safeStorage()?.setItem(REMOTE_URL_KEY, remoteOrigin);
    if (token) setToken(token);
    else clearToken();
  } else if (!switchesOrigin && token) {
    setToken(token);
  }
  if (token || remote) {
    const cleaned = window.location.pathname + window.location.search;
    window.history.replaceState(window.history.state, document.title, cleaned);
  }
}

function parseOrigin(remote: string): string | null {
  try {
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

export function onTokenChange(listener: (token: string) => void): () => void {
  tokenListeners.add(listener);
  return () => tokenListeners.delete(listener);
}

export function setToken(token: string): void {
  const storage = safeStorage();
  if (!storage) return;
  const previous = storage.getItem(STORAGE_KEY);
  if (previous !== token) clearOfflineMail();
  storage.setItem(STORAGE_KEY, token);
  if (previous !== token) for (const listener of tokenListeners) listener(token);
}

export function clearToken(): void {
  clearOfflineMail();
  const storage = safeStorage();
  if (!storage) return;
  storage.removeItem(STORAGE_KEY);
}

export function getBridgeBaseUrl(): string {
  if (typeof window === "undefined") return "";
  const storage = safeStorage();
  const remote = storage?.getItem(REMOTE_URL_KEY) ?? null;
  return remote ?? window.location.origin;
}

export function getBridgeWsUrl(): string {
  const base = getBridgeBaseUrl();
  return base.replace(/^http/, "ws");
}
