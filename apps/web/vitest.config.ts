import { fileURLToPath, URL } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The app formats dates in the viewer's locale, and the tests assert British
// forms ("6 Oct", "09:50"). The forked test workers inherit these, so a
// laptop and a CI runner set to en-US or C format dates the same way.
process.env.LANG = "en_GB.UTF-8";
process.env.LC_ALL = "en_GB.UTF-8";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
    passWithNoTests: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    testTimeout: 15_000,
  },
});
