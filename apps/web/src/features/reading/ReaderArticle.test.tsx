import { act, cleanup, render, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ReaderArticle, useReaderHtml } from "./ReaderArticle";

const HOSTILE =
  `<h2>Real heading</h2><p onclick="steal()">Real text</p>` +
  `<script>window.pwned=1</script><style>body{display:none}</style>` +
  `<a href="javascript:alert(1)">bad link</a><a href="https://example.com">good link</a>` +
  `<img src="https://cdn.example/a.png" onerror="steal()" alt="pic">` +
  `<iframe src="https://evil.example"></iframe><form action="https://evil.example"><input name="x"></form>` +
  `<div style="position:fixed;inset:0" class="fixed inset-0" id="app">overlay</div>`;

const registry = new Map<string, Range[]>();

beforeEach(() => {
  registry.clear();
  vi.stubGlobal("CSS", {
    escape: (value: string) => value,
    highlights: {
      set: (name: string, value: { ranges: Range[] }) => registry.set(name, value.ranges),
      delete: (name: string) => registry.delete(name),
    },
  });
  vi.stubGlobal(
    "Highlight",
    class {
      ranges: Range[];
      constructor(...ranges: Range[]) {
        this.ranges = ranges;
      }
    },
  );
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const nextFrame = () => new Promise((resolve) => requestAnimationFrame(() => resolve(null)));

/** The reader re-tags and re-places highlights on the next animation frame after a render. */
async function settle() {
  await act(async () => {
    await nextFrame();
    await nextFrame();
  });
}

function show(html: string | null, { allowRemoteImages = false, marks = [] as string[] } = {}) {
  const hook = renderHook(() => useReaderHtml(html, allowRemoteImages));
  const clean = hook.result.current;
  if (clean === null) return { clean, view: null };
  const view = render(
    <ReaderArticle
      html={clean}
      paragraphs={[]}
      marks={marks}
      allowRemoteImages={allowRemoteImages}
    />,
  );
  return { clean, view };
}

describe("the reader path for untrusted article HTML", () => {
  test("renders hostile HTML inert", async () => {
    const { view } = show(HOSTILE, { allowRemoteImages: true });
    await settle();
    // SAFETY: ReaderArticle always renders [data-article-body].
    const body = view!.container.querySelector("[data-article-body]") as HTMLElement;
    expect(body.querySelector("script, style, iframe, form, input")).toBeNull();
    expect(body.querySelector("[onclick], [onerror], [class]:not(.reader-prose)")).toBeNull();
    // Only the reader's own paragraph-indent variable may appear in a style attribute.
    for (const el of body.querySelectorAll("[style]"))
      expect(el.getAttribute("style")).toMatch(/^--para-indent: [\d.]+px;?$/);
    expect(body.querySelector("#app")).toBeNull();
    expect([...body.querySelectorAll("a")].map((a) => a.getAttribute("href"))).toEqual([
      null,
      "https://example.com",
    ]);
    expect(body.querySelector("h2")?.textContent).toBe("Real heading");
    // SAFETY: reads a global the hostile markup would have set, if it had run.
    expect((window as unknown as { pwned?: number }).pwned).toBeUndefined();
  });

  test("keeps remote images blocked while remote content is off", async () => {
    const { view } = show(HOSTILE, { allowRemoteImages: false });
    await act(async () => {});
    expect(view!.container.querySelector("img")).toBeNull();
    expect(view!.container.innerHTML).not.toContain("cdn.example");
    expect(view!.container.textContent).toContain("Image blocked");
  });

  test("loads remote images once they are allowed", async () => {
    const { view } = show(HOSTILE, { allowRemoteImages: true });
    await act(async () => {});
    expect(view!.container.querySelector("img")?.getAttribute("src")).toBe(
      "https://cdn.example/a.png",
    );
  });

  test.each([
    ["missing", null],
    ["empty", ""],
    ["blank", "  \n "],
    ["only script", "<script>1</script><style>a{}</style>"],
  ])("falls back to paragraphs when the html is %s", (_name, html) => {
    expect(show(html).clean).toBeNull();
  });

  test("treats a non-string html value from the server as absent", () => {
    // SAFETY: deliberately wrong types, as an unvalidated server response could carry.
    expect(show(42 as unknown as string).clean).toBeNull();
    expect(show({} as unknown as string).clean).toBeNull();
  });

  test("an image-only issue still shows", () => {
    expect(show(`<img src="data:image/png;base64,iVBORw0KGgo=">`).clean).not.toBeNull();
  });

  test("draws highlights on the rendered article, in any storage order", async () => {
    const html = "<p>The bins move to Tuesdays.</p><p>Recycling is unchanged.</p>";
    // Stored newest-first, so the second paragraph's quote comes first.
    const { view } = show(html, {
      marks: ["Recycling is unchanged", "bins move", "gone from article"],
    });
    await settle();
    expect(view).not.toBeNull();
    expect((registry.get("user-highlight") ?? []).map(String).toSorted()).toEqual([
      "Recycling is unchanged",
      "bins move",
    ]);
  });

  test("switching items replaces the highlights instead of keeping the old ones", async () => {
    const first = show("<p>alpha beta</p>", { marks: ["alpha"] });
    await settle();
    expect(registry.get("user-highlight")?.map(String)).toEqual(["alpha"]);
    first.view!.unmount();
    expect(registry.has("user-highlight")).toBe(false);

    show("<p>gamma delta</p>", { marks: [] });
    await settle();
    expect(registry.get("user-highlight") ?? []).toHaveLength(0);
  });
});
