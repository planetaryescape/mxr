import { useCallback, useSyncExternalStore } from "react";

import { clearToken, getToken, setToken } from "@/lib/tokenStorage";

let listeners = new Set<() => void>();

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function notify() {
  for (const cb of listeners) cb();
}

function getSnapshot() {
  return getToken() ?? "";
}

export function useBridgeToken(): {
  token: string;
  setToken: (t: string) => void;
  clearToken: () => void;
  hasToken: boolean;
} {
  const token = useSyncExternalStore(subscribe, getSnapshot, () => "");
  const setT = useCallback((t: string) => {
    setToken(t);
    notify();
  }, []);
  const clearT = useCallback(() => {
    clearToken();
    notify();
  }, []);
  return { token, setToken: setT, clearToken: clearT, hasToken: token.length > 0 };
}
