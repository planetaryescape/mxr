import "@testing-library/jest-dom/vitest";

import { beforeEach } from "vitest";

// jsdom gives a test *file* one Storage instance, so anything a component
// persists survives into every later test in that file. Compose remembers its
// active draft that way, which sent a later test down the "resurrect the saved
// draft" path against a mock that was never stubbed for it.
//
// This only failed on CI because Node 26 ships its own inert `localStorage`
// (the "--localstorage-file was not provided" warning) that masks the leak,
// while Node 24 — the version CI runs — uses jsdom's working one.
beforeEach(() => {
  // The DOM lib types `window` as always present, but test files that opt into
  // the node environment have no such global.
  if (typeof window === "undefined") return;
  try {
    window.localStorage.clear();
    window.sessionStorage.clear();
  } catch {
    // Opaque test origins disable storage entirely; nothing to clear.
  }
});
