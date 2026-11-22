import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { apiFetchBlob } from "@/api/client";

export function inlineImageSources(html: string): string[] {
  if (typeof DOMParser === "undefined") return [];
  const doc = new DOMParser().parseFromString(html, "text/html");
  const sources = Array.from(doc.querySelectorAll("img[src]"))
    .map((image) => image.getAttribute("src") ?? "")
    .filter((src) => /^cid:/i.test(src.trim()));
  return [...new Set(sources)];
}

function toDataUri(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => resolve(String(reader.result)));
    reader.addEventListener("error", () =>
      reject(reader.error ?? new Error("could not read image")),
    );
    reader.readAsDataURL(blob);
  });
}

export function replaceImageSources(html: string, replacements: Map<string, string>): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const image of Array.from(doc.querySelectorAll("img[src]"))) {
    const src = image.getAttribute("src") ?? "";
    if (!/^cid:/i.test(src.trim())) continue;
    const replacement = replacements.get(src);
    if (replacement) image.setAttribute("src", replacement);
    else image.removeAttribute("src");
  }
  return doc.body.innerHTML;
}

export function useInlineImages(messageId: string | undefined, html: string | null): string | null {
  const sources = useMemo(() => (html ? inlineImageSources(html) : []), [html]);
  const images = useQuery({
    queryKey: ["inline-images", messageId, sources],
    enabled: Boolean(messageId && sources.length > 0),
    staleTime: Infinity,
    queryFn: async ({ signal }) => {
      const settled = await Promise.allSettled(
        sources.map(async (source) => {
          const blob = await apiFetchBlob(
            `/api/v1/mail/messages/${encodeURIComponent(messageId ?? "")}/inline-image?source=${encodeURIComponent(source)}`,
            { signal },
          );
          return [source, await toDataUri(blob)] as const;
        }),
      );
      return new Map(
        settled.flatMap((result) => (result.status === "fulfilled" ? [result.value] : [])),
      );
    },
  });
  return useMemo(() => {
    if (!html || sources.length === 0) return html;
    return replaceImageSources(html, images.data ?? new Map());
  }, [html, images.data, sources.length]);
}
