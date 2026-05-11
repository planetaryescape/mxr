import { useEffect, useMemo, useRef, useState } from "react";

import { sanitizeHtml } from "@/lib/sanitizeHtml";
import { cn } from "@/lib/utils";
import type { EmailHtmlTheme } from "@/state/uiPrefsStore";

interface MessageBodyProps {
  html: string;
  allowRemoteImages?: boolean;
  theme?: EmailHtmlTheme;
  fillAvailable?: boolean;
}

const IFRAME_SANDBOX = "allow-popups allow-popups-to-escape-sandbox";

export function MessageBody({
  html,
  allowRemoteImages = true,
  theme = "dark",
  fillAvailable = false,
}: MessageBodyProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(320);
  const [loaded, setLoaded] = useState(false);
  const srcDoc = useMemo(
    () => renderHtmlDocument(html, allowRemoteImages, theme),
    [allowRemoteImages, html, theme],
  );
  const frameBackground = theme === "dark" ? "#11110f" : "#fff";

  useEffect(() => setLoaded(false), [srcDoc]);

  function resizeToContent() {
    try {
      const body = iframeRef.current?.contentDocument?.body;
      if (body) setHeight(Math.max(160, body.scrollHeight));
    } catch {
      setHeight(320);
    } finally {
      setLoaded(true);
    }
  }

  return (
    <div
      className={cn(
        "overflow-hidden rounded-md border border-border",
        fillAvailable && "h-full min-h-0",
      )}
      style={{ backgroundColor: frameBackground }}
    >
      <iframe
        ref={iframeRef}
        title="HTML message body"
        className={cn("block min-h-40 w-full border-0", fillAvailable && "h-full")}
        sandbox={IFRAME_SANDBOX}
        srcDoc={srcDoc}
        style={{
          height: fillAvailable ? "100%" : height,
          backgroundColor: frameBackground,
          colorScheme: theme === "dark" ? "dark" : "light",
          opacity: loaded ? 1 : 0,
          transition: "opacity 80ms ease-out",
        }}
        onLoad={resizeToContent}
      />
    </div>
  );
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
