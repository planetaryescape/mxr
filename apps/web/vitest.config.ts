import { fileURLToPath, URL } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

import { mxrVersion } from "./scripts/mxr-version";
import { keymapModules } from "./src/lib/actions/keymapPlugin";

process.env.LANG = "en_GB.UTF-8";
process.env.LC_ALL = "en_GB.UTF-8";
process.env.TZ = "Europe/London";

export default defineConfig({
  define: {
    MXR_BUILD_VERSION: JSON.stringify(mxrVersion()),
  },
  plugins: [keymapModules(), react()],
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
