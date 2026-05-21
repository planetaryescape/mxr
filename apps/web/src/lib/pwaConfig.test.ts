import { describe, expect, it } from "vitest";

import { pwaManifest, pwaOptions } from "./pwaConfig";

describe("PWA config", () => {
  it("is installable as a standalone app", () => {
    expect(pwaManifest.name).toBe("mxr");
    expect(pwaManifest.short_name).toBe("mxr");
    expect(pwaManifest.start_url).toBe("/");
    expect(pwaManifest.scope).toBe("/");
    expect(pwaManifest.display).toBe("standalone");
    expect(pwaManifest.icons).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ src: "/pwa-192.png", sizes: "192x192" }),
        expect.objectContaining({ src: "/pwa-512.png", sizes: "512x512" }),
        expect.objectContaining({ src: "/pwa-maskable-512.png", purpose: "maskable" }),
      ]),
    );
  });

  it("retires stale service workers before they can outlive the daemon protocol", () => {
    expect(pwaOptions.registerType).toBe("autoUpdate");
    expect(pwaOptions.selfDestroying).toBe(true);
  });

  it("keeps API traffic out of the app-shell fallback if PWA caching returns", () => {
    expect(pwaOptions.workbox?.globPatterns).toEqual(["**/*.{js,css,html,png,svg,woff2}"]);

    const denylist = pwaOptions.workbox?.navigateFallbackDenylist ?? [];
    expect(denylist.some((pattern) => pattern.test("/api/v1/mail/mailbox"))).toBe(true);
    expect(denylist.every((pattern) => !pattern.test("/m/inbox"))).toBe(true);
  });
});
