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

import { highlightSelection } from "./readingVerbs";

describe("highlighting with nothing selected", () => {
  beforeEach(() => saveHighlight.mockReset().mockResolvedValue({}));

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
});
