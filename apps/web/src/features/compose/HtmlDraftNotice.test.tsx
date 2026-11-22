/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { HtmlDraftNotice } from "./HtmlDraftNotice";

function renderNotice(previewHtml: string) {
  render(<HtmlDraftNotice refusal={{ previewHtml }} />);
  const frame = screen.getByTitle("HTML message body");
  expect(frame).toBeInstanceOf(HTMLIFrameElement);
  return frame as HTMLIFrameElement;
}

describe("HtmlDraftNotice preview", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    cleanup();
    vi.clearAllTimers();
    vi.useRealTimers();
  });
  test("renders the draft inside a sandboxed iframe", () => {
    const frame = renderNotice("<p>Ship on Friday.</p>");

    const sandbox = frame.getAttribute("sandbox");
    expect(sandbox).not.toBeNull();
    expect(sandbox).not.toContain("allow-scripts");
    expect(sandbox).not.toContain("allow-forms");
    expect(frame.getAttribute("srcdoc")).toContain("Ship on Friday.");
  });

  test("does not let the preview request a remote image", () => {
    const frame = renderNotice(
      '<p>hello</p><img src="https://tracker.example.com/pixel.png?id=42" alt="pixel">',
    );

    const srcDoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcDoc).toContain("hello");
    const preview = new DOMParser().parseFromString(srcDoc, "text/html");
    const fetched = [...preview.querySelectorAll("[src], [srcset], [href], [background]")].filter(
      (node) =>
        ["src", "srcset", "background"].some((attribute) =>
          /^https?:/i.test(node.getAttribute(attribute) ?? ""),
        ),
    );
    expect(fetched).toEqual([]);
  });

  test("does not let the preview run a script", () => {
    const frame = renderNotice("<p>hello</p><script>window.leaked = 1;</script>");

    const srcDoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcDoc).toContain("hello");
    expect(srcDoc).not.toContain("window.leaked");
  });
});
