/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { htmlDraftRefusal, HtmlDraftNotice } from "./HtmlDraftNotice";

function renderNotice(previewHtml: string) {
  render(<HtmlDraftNotice refusal={{ previewHtml }} />);
  const frame = screen.getByTitle("HTML message body");
  expect(frame).toBeInstanceOf(HTMLIFrameElement);
  return frame as HTMLIFrameElement;
}

describe("htmlDraftRefusal", () => {
  test("recognises the bridge refusal and carries the document through", () => {
    const error = Object.assign(new Error("this draft has an HTML body"), {
      code: "html_draft_not_editable",
      details: { previewHtml: "<p>hi</p>" },
    });

    expect(htmlDraftRefusal(error)).toEqual({ previewHtml: "<p>hi</p>" });
  });

  test("ignores every other failure so the caller keeps its retry", () => {
    expect(htmlDraftRefusal(new Error("daemon is not running"))).toBeNull();
    expect(
      htmlDraftRefusal(Object.assign(new Error("nope"), { code: "some_other_code" })),
    ).toBeNull();
    expect(htmlDraftRefusal(null)).toBeNull();
    expect(htmlDraftRefusal("html_draft_not_editable")).toBeNull();
  });
});

describe("HtmlDraftNotice preview", () => {
  test("renders the draft inside a sandboxed iframe", () => {
    const frame = renderNotice("<p>Ship on Friday.</p>");

    const sandbox = frame.getAttribute("sandbox");
    expect(sandbox).not.toBeNull();
    // Scripts and form submission must stay off: the same posture the thread
    // reader uses for untrusted mail.
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
    // Nothing in the document may still name a remote URL in an attribute the
    // browser fetches. (DOMPurify parks the original in `data-original-src`,
    // which is inert until the reader explicitly unblocks images.)
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

  test("names an empty body instead of rendering a blank panel", () => {
    render(<HtmlDraftNotice refusal={{ previewHtml: "" }} />);

    expect(screen.queryByTitle("HTML message body")).not.toBeInTheDocument();
    expect(screen.getByText(/HTML body is empty/i)).toBeVisible();
  });

  test("tells the user how to edit an HTML draft", () => {
    renderNotice("<p>body</p>");

    expect(screen.getByText(/This draft has an HTML body/i)).toBeInTheDocument();
    expect(screen.getByText(/--html-file/)).toBeInTheDocument();
  });
});
