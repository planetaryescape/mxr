import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: resolve(__dirname, "src/renderer/test-setup.ts"),
    exclude: ["out/**", "dist/**", "dist-electron/**", "node_modules/**"],
  },
});
