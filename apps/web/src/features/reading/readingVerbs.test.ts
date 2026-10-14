import { beforeEach, describe, expect, it, vi } from "vitest";

const sendToMarginalia = vi.hoisted(() => vi.fn<(key: string) => Promise<unknown>>());
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  sendToMarginalia,
}));
vi.mock("@/lib/daemonAvailability", () => ({
  refuseWhileDaemonDown: () => false,
}));
vi.mock("sonner", () => ({
  toast: Object.assign(vi.fn<() => void>(), {
    loading: vi.fn<() => void>(),
    success: vi.fn<() => void>(),
    error: vi.fn<() => void>(),
    message: vi.fn<() => void>(),
  }),
}));

import { isWebUrl, openInMarginalia } from "./readingVerbs";

describe("opening Marginalia", () => {
  beforeEach(() => vi.restoreAllMocks());

  it("accepts only http and https addresses", () => {
    expect(isWebUrl("https://reader.example.com/read/k57")).toBe(true);
    expect(isWebUrl("http://localhost:3000/read/k57")).toBe(true);
    for (const bad of [
      "javascript:alert(1)",
      "data:text/html,hi",
      "file:///etc/passwd",
      "/read/x",
      "",
    ]) {
      expect(isWebUrl(bad)).toBe(false);
    }
  });

  it("closes the pre-opened tab instead of navigating it to a script URL", async () => {
    const tab = {
      opener: {},
      close: vi.fn<() => void>(),
      location: { href: "about:blank" },
    };
    vi.spyOn(window, "open").mockReturnValue(tab as unknown as Window);
    sendToMarginalia.mockResolvedValue({
      copy: "Sent to Marginalia.",
      marginalia: { read_url: "javascript:alert(1)" },
    });
    await openInMarginalia("k", null, {
      connected: true,
      onConnect: () => undefined,
    });
    expect(tab.location.href).toBe("about:blank");
    expect(tab.close).toHaveBeenCalledOnce();
  });

  it("navigates the pre-opened tab to a web address", async () => {
    const tab = {
      opener: {},
      close: vi.fn<() => void>(),
      location: { href: "about:blank" },
    };
    vi.spyOn(window, "open").mockReturnValue(tab as unknown as Window);
    sendToMarginalia.mockResolvedValue({
      copy: "Sent to Marginalia.",
      marginalia: { read_url: "https://reader.example.com/read/k57" },
    });
    await openInMarginalia("k", null, {
      connected: true,
      onConnect: () => undefined,
    });
    expect(tab.location.href).toBe("https://reader.example.com/read/k57");
    expect(tab.close).not.toHaveBeenCalled();
  });
});
