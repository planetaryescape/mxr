import "@planetaryescape/reader/reader.css";

import {
  ArticleRenderer,
  useHighlights,
  useParagraphs,
  type HighlightDoc,
} from "@planetaryescape/reader";
import { useMemo, useRef } from "react";

import { sanitizeReaderHtml } from "@/lib/sanitizeHtml";

import type { ReadingParagraph } from "./api";
import type { AnchoredMark } from "./readingView";

const NO_ANNOTATIONS: never[] = [];
const noop = () => {};

function sanitizeFor(allowRemoteImages: boolean) {
  return (html: string) => sanitizeReaderHtml(html, { allowRemoteImages });
}

/** Whether sanitized markup would show the reader anything. */
function hasVisibleContent(html: string): boolean {
  const body = new DOMParser().parseFromString(html, "text/html").body;
  return Boolean(body.textContent?.trim()) || body.querySelector("img, hr") !== null;
}

/**
 * The HTML to show for an issue or article, sanitized, or null when there is
 * none (the daemon sent no HTML, or nothing survived sanitizing) and the
 * reader should fall back to the plain paragraphs. The HTML is untrusted: it
 * goes through the reader sanitizer, and `allowRemoteImages` gates images the
 * way email does.
 */
export function useReaderHtml(
  html: string | null | undefined,
  allowRemoteImages: boolean,
): string | null {
  return useMemo(() => {
    // The bridge response is not validated at runtime; a non-string must not reach the sanitizer.
    if (typeof html !== "string" || !html.trim()) return null;
    const clean = sanitizeFor(allowRemoteImages)(html);
    return hasVisibleContent(clean) ? clean : null;
  }, [html, allowRemoteImages]);
}

/**
 * Sanitized article HTML, inline, in the reader's typography, with the quotes
 * in `marks` drawn over the text by matching them in the rendered article.
 * `html` should come from `useReaderHtml`; the renderer sanitizes it once more
 * because it trusts nothing but its `sanitize` prop.
 */
export function ReaderArticle({
  html,
  paragraphs,
  marks,
  allowRemoteImages,
}: {
  html: string;
  paragraphs: ReadingParagraph[];
  marks: AnchoredMark[];
  allowRemoteImages: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const sanitize = useMemo(() => sanitizeFor(allowRemoteImages), [allowRemoteImages]);
  const text = useMemo(
    () => paragraphs.map((paragraph) => paragraph.text).join("\n\n"),
    [paragraphs],
  );
  const docs = useMemo<HighlightDoc[]>(
    () =>
      marks.map(({ quote, paragraph }) => ({
        text: quote,
        kind: "highlight",
        // Absent, the package matches the whole article, as for older highlights.
        ...(typeof paragraph === "number" ? { paragraphIndex: paragraph } : {}),
      })),
    [marks],
  );
  const { version } = useParagraphs(ref, true);
  useHighlights({ containerRef: ref, docs, version });

  return (
    <div ref={ref} data-testid="reader-text" data-format="html" className="mt-6">
      <ArticleRenderer
        content={text}
        contentHtml={html}
        sanitize={sanitize}
        annotations={NO_ANNOTATIONS}
        activeAnnotationId={null}
        onAnnotationSelect={noop}
      />
    </div>
  );
}
