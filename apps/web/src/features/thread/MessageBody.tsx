import { useEffect, useMemo, useRef, useState } from "react";

import { sanitizeHtml } from "@/lib/sanitizeHtml";
import type { EmailHtmlTheme } from "@/state/uiPrefsStore";

interface MessageBodyProps {
  html: string;
  allowRemoteImages?: boolean;
  theme?: EmailHtmlTheme;
}

const IFRAME_SANDBOX = "allow-same-origin allow-popups allow-popups-to-escape-sandbox";

export function MessageBody({ html, allowRemoteImages = true, theme = "dark" }: MessageBodyProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const [height, setHeight] = useState(320);
  const [loaded, setLoaded] = useState(false);
  const srcDoc = useMemo(
    () => renderHtmlDocument(html, allowRemoteImages, theme),
    [allowRemoteImages, html, theme],
  );
  const frameBackground = theme === "dark" ? "#11110f" : "#fff";

  useEffect(() => {
    setLoaded(false);
    resizeObserverRef.current?.disconnect();
    resizeObserverRef.current = null;
  }, [srcDoc]);

  useEffect(
    () => () => {
      resizeObserverRef.current?.disconnect();
    },
    [],
  );

  function resizeToContent() {
    try {
      const doc = iframeRef.current?.contentDocument;
      const body = doc?.body;
      const root = doc?.documentElement;
      if (body || root) {
        setHeight(
          Math.max(
            160,
            body?.scrollHeight ?? 0,
            body?.offsetHeight ?? 0,
            root?.scrollHeight ?? 0,
            root?.offsetHeight ?? 0,
          ),
        );
      }
    } catch {
      setHeight(320);
    } finally {
      setLoaded(true);
    }
  }

  function handleLoad() {
    resizeToContent();
    try {
      const doc = iframeRef.current?.contentDocument;
      const body = doc?.body;
      if (body && "ResizeObserver" in window) {
        resizeObserverRef.current?.disconnect();
        resizeObserverRef.current = new ResizeObserver(resizeToContent);
        resizeObserverRef.current.observe(body);
      }
      doc?.addEventListener("click", handleLinkClick);
    } catch {
      // The iframe still renders; fixed fallback height comes from resizeToContent.
    }
    window.setTimeout(resizeToContent, 100);
    window.setTimeout(resizeToContent, 500);
  }

  return (
    <div
      className="overflow-hidden rounded-md border border-border"
      style={{ backgroundColor: frameBackground }}
    >
      <iframe
        ref={iframeRef}
        title="HTML message body"
        className="block min-h-40 w-full border-0"
        sandbox={IFRAME_SANDBOX}
        srcDoc={srcDoc}
        style={{
          height,
          backgroundColor: frameBackground,
          colorScheme: theme === "dark" ? "dark" : "light",
          opacity: loaded ? 1 : 0,
          transition: "opacity 80ms ease-out",
        }}
        onLoad={handleLoad}
      />
    </div>
  );
}

function handleLinkClick(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const anchor = target.closest("a[href]");
  if (!(anchor instanceof HTMLAnchorElement)) return;
  const href = anchor.href || anchor.getAttribute("href") || "";
  if (!safeExternalHref(href)) return;
  event.preventDefault();
  event.stopPropagation();
  window.open(href, "_blank", "noopener,noreferrer");
}

function safeExternalHref(href: string): boolean {
  try {
    const url = new URL(href);
    return ["http:", "https:", "mailto:", "tel:"].includes(url.protocol);
  } catch {
    return false;
  }
}

function renderHtmlDocument(
  html: string,
  allowRemoteImages: boolean,
  theme: EmailHtmlTheme,
): string {
  const sanitized = sanitizeHtml(html, {
    allowRemoteImages,
    stripLightBackgrounds: theme === "dark",
    stripDarkTextColors: theme === "dark",
  });
  const style = theme === "dark" ? darkEmailCss : originalEmailCss;
  return `<!doctype html><html><head><base target="_blank"><meta name="color-scheme" content="${theme === "dark" ? "dark" : "light"}"><style>${style}</style></head><body>${sanitized}</body></html>`;
}

const originalEmailCss = `html{color-scheme:light;background:#f7f7f5}body{box-sizing:border-box;max-width:860px;margin:0 auto;padding:24px 32px;font:14px/1.5 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#111;background:#fff;overflow-wrap:anywhere}*,*:before,*:after{box-sizing:border-box}table{max-width:100%;border-collapse:collapse}body>table{margin-left:auto;margin-right:auto}img{max-width:100%;height:auto}a[href]{color:#0369a1!important;text-decoration:underline!important;text-underline-offset:2px}a[href]:hover{color:#075985!important}`;

const darkEmailCss = `html{color-scheme:dark;background:#11110f}body{box-sizing:border-box;max-width:860px;margin:0 auto;padding:24px 32px;font:14px/1.55 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#e8dfcf;background:#11110f;overflow-wrap:anywhere}*,*:before,*:after{box-sizing:border-box}table{max-width:100%;border-collapse:collapse}body>table{margin-left:auto;margin-right:auto}a[href]{color:#9cc9ff!important;text-decoration:underline!important;text-underline-offset:2px}a[href]:hover{color:#c7ddff!important}hr{border:0;border-top:1px solid #3a352e}pre,code,kbd,samp{background:#1c1915;color:#f4ead9;border-radius:4px}pre{padding:12px;white-space:pre-wrap}blockquote{border-left:3px solid #4b4338;margin-left:0;padding-left:12px;color:#d5c8b7}mark{background:#4a3b12;color:#f5e9c8}img{max-width:100%;height:auto;filter:brightness(.96) contrast(.99)}`;
