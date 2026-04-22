import "@testing-library/jest-dom/vitest";
import { afterAll, afterEach, beforeAll } from "vitest";

function createMemoryStorage(): Storage {
  const entries = new Map<string, string>();

  return {
    get length() {
      return entries.size;
    },
    clear() {
      entries.clear();
    },
    getItem(key: string) {
      return entries.get(key) ?? null;
    },
    key(index: number) {
      return [...entries.keys()][index] ?? null;
    },
    removeItem(key: string) {
      entries.delete(key);
    },
    setItem(key: string, value: string) {
      entries.set(key, String(value));
    },
  };
}

Object.defineProperty(globalThis, "localStorage", {
  value: createMemoryStorage(),
  configurable: true,
});

Object.defineProperty(globalThis, "sessionStorage", {
  value: createMemoryStorage(),
  configurable: true,
});

const { desktopMockServer, resetDesktopMockServer } =
  await import("./test/desktopMockServer");

beforeAll(() => {
  desktopMockServer.listen({ onUnhandledRequest: "error" });
});

afterEach(() => {
  desktopMockServer.resetHandlers();
  resetDesktopMockServer();
});

afterAll(() => {
  desktopMockServer.close();
});
