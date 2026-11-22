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

async function settle() {
  await act(async () => {
    await nextFrame();
    await nextFrame();
  });
}

function show(
  html: string | null,
  { allowRemoteImages = false, marks = [] as { quote: string; paragraph?: number }[] } = {},
) {
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

  test("falls back to paragraphs when the html is missing, blank, script-only or not a string", () => {
    // SAFETY: deliberately wrong types, as an unvalidated server response could carry.
    expect(show(null).clean).toBeNull();
    expect(show("  \n ").clean).toBeNull();
    expect(show("<script>1</script><style>a{}</style>").clean).toBeNull();
    expect(show(42 as unknown as string).clean).toBeNull();
    expect(show({} as unknown as string).clean).toBeNull();
  });
});
