import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, URL } from "node:url";

import tailwind from "@tailwindcss/vite";
import { TanStackRouterVite } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { visualizer } from "rollup-plugin-visualizer";
import { defineConfig } from "vite";

/**
 * Discover the bridge port the daemon last bound to.
 *
 * Precedence:
 *   1. `MXR_BRIDGE_URL` env override (full URL).
 *   2. `MXR_BRIDGE_PORT_PATH` or `MXR_CONFIG_DIR/bridge-port`.
 *   3. Platform-conventional config dirs:
 *        - Linux:   `~/.config/mxr/bridge-port`
 *        - macOS:   `~/Library/Application Support/mxr/bridge-port`
 *        - Windows: `%APPDATA%\mxr\bridge-port`
 *   4. Built-in default: `127.0.0.1:42829`.
 */
function configCandidates(): string[] {
  if (process.env.MXR_BRIDGE_PORT_PATH) return [process.env.MXR_BRIDGE_PORT_PATH];
  if (process.env.MXR_CONFIG_DIR) {
    return [join(process.env.MXR_CONFIG_DIR, "bridge-port")];
  }
  const home = homedir();
  const candidates: string[] = [];
  if (process.platform === "darwin") {
    candidates.push(join(home, "Library", "Application Support", "mxr", "bridge-port"));
  } else if (process.platform === "win32") {
    const appdata = process.env.APPDATA ?? join(home, "AppData", "Roaming");
    candidates.push(join(appdata, "mxr", "bridge-port"));
  } else {
    candidates.push(join(home, ".config", "mxr", "bridge-port"));
  }
  // Also check the cross-platform fallback so users with explicit
  // `MXR_CONFIG_DIR` setups still work even if Vite missed the env.
  candidates.push(join(home, ".config", "mxr", "bridge-port"));
  return candidates;
}

function resolveBridgeTarget(): string {
  if (process.env.MXR_BRIDGE_URL) return process.env.MXR_BRIDGE_URL;
  for (const portFile of configCandidates()) {
    if (!existsSync(portFile)) continue;
    const raw = readFileSync(portFile, "utf8").trim();
    const port = Number.parseInt(raw, 10);
    if (Number.isInteger(port) && port > 0 && port < 65_536) {
      return `http://127.0.0.1:${port}`;
    }
  }
  return "http://127.0.0.1:42829";
}

const BRIDGE_TARGET = resolveBridgeTarget();
const BRIDGE_WS_TARGET = BRIDGE_TARGET.replace(/^http/, "ws");

export default defineConfig({
  plugins: [
    TanStackRouterVite({
      target: "react",
      routesDirectory: "./src/routes",
      generatedRouteTree: "./src/routeTree.gen.ts",
      autoCodeSplitting: true,
    }),
    react(),
    tailwind(),
    process.env.ANALYZE === "1" &&
      visualizer({ filename: "dist/stats.html", template: "treemap", gzipSize: true }),
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
    cssCodeSplit: true,
    rollupOptions: {
      output: {
        manualChunks: (id) => {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("node_modules/react") || id.includes("node_modules/react-dom"))
            return "vendor-react";
          if (id.includes("node_modules/@tanstack")) return "vendor-tanstack";
          if (id.includes("node_modules/@radix-ui") || id.includes("node_modules/cmdk"))
            return "vendor-radix";
          if (id.includes("node_modules/recharts")) return "vendor-recharts";
          if (id.includes("node_modules/@tiptap")) return "vendor-tiptap";
          if (id.includes("node_modules/@replit/codemirror-vim")) return "vendor-codemirror-vim";
          if (id.includes("node_modules/@codemirror/view")) return "vendor-codemirror-view";
          if (id.includes("node_modules/@codemirror/state")) return "vendor-codemirror-state";
          if (id.includes("node_modules/codemirror") || id.includes("node_modules/@codemirror"))
            return "vendor-codemirror-core";
          if (id.includes("node_modules/@lezer")) return "vendor-codemirror-core";
          if (id.includes("node_modules/lucide-react")) return "vendor-icons";
        },
      },
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": {
        target: BRIDGE_TARGET,
        changeOrigin: true,
        secure: false,
      },
      "/api/v1/events": {
        target: BRIDGE_WS_TARGET,
        ws: true,
        changeOrigin: true,
      },
    },
  },
});
