/* @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { bootstrapFromHash, getBridgeBaseUrl, getToken, setToken } from "./tokenStorage";

// Node 26 shadows jsdom's storage with an inert global, so the test brings
// its own to behave the same on every Node version.
function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, String(value)),
  };
}

beforeEach(() => {
  vi.stubGlobal("localStorage", memoryStorage());
  window.history.replaceState({}, "", "/");
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("bootstrapFromHash", () => {
  test("a link cannot silently point the app at another bridge", () => {
    setToken("local-secret");
    window.history.replaceState({}, "", "/#remote=https://evil.example");
    const confirm = vi.fn<(origin: string) => boolean>(() => false);

    bootstrapFromHash(confirm);

    expect(confirm).toHaveBeenCalledWith("https://evil.example");
    expect(getBridgeBaseUrl()).toBe(window.location.origin);
    expect(getToken()).toBe("local-secret");
    expect(window.location.hash).toBe("");
  });

  test("an accepted remote never carries the local token to it", () => {
    setToken("local-secret");
    window.history.replaceState({}, "", "/#remote=https://mail.example");

    bootstrapFromHash(() => true);

    expect(getBridgeBaseUrl()).toBe("https://mail.example");
    expect(getToken()).toBeUndefined();
  });

  test("a same-origin token link is stored without asking", () => {
    window.history.replaceState({}, "", "/#token=abc");
    const confirm = vi.fn<(origin: string) => boolean>(() => true);

    bootstrapFromHash(confirm);

    expect(confirm).not.toHaveBeenCalled();
    expect(getToken()).toBe("abc");
  });
});
