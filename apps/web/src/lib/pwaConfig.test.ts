import { describe, expect, it } from "vitest";

import { pwaManifest } from "./pwaConfig";

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
});
