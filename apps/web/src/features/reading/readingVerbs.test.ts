import { beforeEach, describe, expect, it, vi } from "vitest";

const sendToMarginalia = vi.hoisted(() => vi.fn<(key: string) => Promise<unknown>>());
const saveHighlight = vi.hoisted(() => vi.fn<(input: unknown) => Promise<unknown>>());
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  sendToMarginalia,
  saveHighlight,
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
    info: vi.fn<() => void>(),
  }),
}));

import { toast } from "sonner";

import { highlightSelection, isWebUrl, openInMarginalia } from "./readingVerbs";

describe("highlighting with nothing selected", () => {
  beforeEach(() => saveHighlight.mockReset().mockResolvedValue({}));

  it("names the highlight key from the key definition, not the retired h", async () => {
    expect(await highlightSelection("", "item-1", "issue")).toBe(false);
    expect(toast.info).toHaveBeenCalledWith(
      'Scroll to a paragraph or select some text, then press "',
    );
    expect(saveHighlight).not.toHaveBeenCalled();
  });

  it("saves the paragraph at the top, anchored to it, and names it", async () => {
    const top = {
      index: 3,
      text: "A delete is the absence of a row, which is why undo needs a journal.",
    };
    expect(await highlightSelection("", "item-1", "article", undefined, null, top)).toBe(true);
    expect(saveHighlight).toHaveBeenCalledWith({
      itemKey: "item-1",
      view: "article",
      quote: top.text,
      note: undefined,
      paragraph: 3,
    });
    expect(toast.success).toHaveBeenCalledWith("Highlighted “A delete is the absence of a row,…”");
  });

  it("prefers a selection over the paragraph at the top", async () => {
    await highlightSelection("the absence", "item-1", "issue", undefined, 5, {
      index: 3,
      text: "Something else entirely.",
    });
    expect(saveHighlight).toHaveBeenCalledWith(
      expect.objectContaining({ quote: "the absence", paragraph: 5 }),
    );
    expect(toast.success).toHaveBeenCalledWith("Highlight saved");
  });
});

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
