import type { ManifestOptions, VitePWAOptions } from "vite-plugin-pwa";

export const pwaManifest: Partial<ManifestOptions> = {
  id: "/",
  name: "mxr",
  short_name: "mxr",
  description: "Local-first mail in the browser, backed by the mxr daemon.",
  start_url: "/",
  scope: "/",
  display: "standalone",
  background_color: "#0e1116",
  theme_color: "#0e1116",
  icons: [
    {
      src: "/pwa-192.png",
      sizes: "192x192",
      type: "image/png",
    },
    {
      src: "/pwa-512.png",
      sizes: "512x512",
      type: "image/png",
    },
    {
      src: "/pwa-maskable-512.png",
      sizes: "512x512",
      type: "image/png",
      purpose: "maskable",
    },
  ],
};

export const pwaOptions: Partial<VitePWAOptions> = {
  strategies: "injectManifest",
  srcDir: "src/pwa",
  filename: "service-worker.ts",
  registerType: "prompt",
  includeAssets: ["favicon.svg", "pwa-192.png", "pwa-512.png", "pwa-maskable-512.png"],
  manifest: pwaManifest,
  injectManifest: {
    globPatterns: ["**/*.{js,css,html,png,svg,woff2}"],
  },
};
