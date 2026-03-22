import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  base: "./",
  root: resolve(__dirname, "src/renderer"),
  plugins: [react(), tailwindcss()],
  build: {
    outDir: resolve(__dirname, "dist/renderer"),
    emptyOutDir: true,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: resolve(__dirname, "src/renderer/test-setup.ts"),
  },
});
