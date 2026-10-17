import "@testing-library/jest-dom/vitest";

import { privateStorage } from "./privateStorage";
import { afterEach, beforeEach, vi } from "vitest";

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
    if (!window.localStorage || typeof window.localStorage.setItem !== "function")
      vi.stubGlobal("localStorage", privateStorage());
    window.localStorage.clear();
    window.sessionStorage.clear();
  } catch {
    // Opaque test origins disable storage entirely; nothing to clear.
  }
});

// Radix's FocusScope (every modal dialog and sheet) queues a 0ms timer when it
// unmounts, and that timer builds a CustomEvent and dispatches it on the
// container. Testing Library unmounts after each test, so for a file's last test
// the timer could fire after vitest tore jsdom down, when CustomEvent is no
// longer jsdom's: "Failed to execute 'dispatchEvent' ... not of type 'Event'",
// an unhandled error that fails the run (main 2411a315b). Waiting one macrotask
// after cleanup lets it fire while jsdom is still there. This hook is registered
// before Testing Library's, and vitest runs after-hooks in reverse, so it runs
// after the unmount.
afterEach(async () => {
  if (typeof window === "undefined") return;
  await new Promise((resolve) => setTimeout(resolve, 0));
});
