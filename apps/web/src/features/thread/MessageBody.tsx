import { useEffect, useMemo, useRef, useState } from "react";

import { sanitizeHtml } from "@/lib/sanitizeHtml";
import { useUiPrefs, type EmailHtmlTheme } from "@/state/uiPrefsStore";

import { ASK_MARK_ATTRIBUTE, markQuoteInDocument } from "./context/askQuote";

interface MessageBodyProps {
  html: string;
  allowRemoteImages?: boolean;
  theme?: EmailHtmlTheme;
  /**
   * The ask's quote. Marked by walking the sanitized frame's text nodes, so
   * no markup is ever built from it.
   */
  highlight?: string;
  /** Clicks inside the frame never reach the page; this reports them. */
  onInteract?: () => void;
}

const IFRAME_SANDBOX = "allow-same-origin allow-popups allow-popups-to-escape-sandbox";

export function MessageBody({
  html,
  allowRemoteImages = false,
  theme = "dark",
  highlight,
  onInteract,
}: MessageBodyProps) {
  const onInteractRef = useRef(onInteract);
  onInteractRef.current = onInteract;
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const [height, setHeight] = useState(120);
  const [loaded, setLoaded] = useState(false);
  const appTheme = useUiPrefs((s) => s.theme);
  // "Dark" email rendering only makes sense under a dark app theme.
  const effectiveTheme: EmailHtmlTheme =
    theme === "dark" && document.documentElement.dataset.scheme === "dark" ? "dark" : "original";
  const palette = useMemo(() => readPalette(), [appTheme]); // eslint-disable-line react-hooks/exhaustive-deps
  const srcDoc = useMemo(
    () => renderHtmlDocument(html, allowRemoteImages, effectiveTheme, palette),
    [allowRemoteImages, html, effectiveTheme, palette],
  );
  const frameBackground = effectiveTheme === "dark" ? palette.background : "#ffffff";

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

  // The gist can land after the frame has loaded; mark (or unmark) then.
  useEffect(() => {
    if (!loaded) return;
    const body = iframeRef.current?.contentDocument?.body;
    if (body) markQuoteInDocument(body, highlight ?? "");
  }, [highlight, loaded]);

  function resizeToContent() {
    try {
      const doc = iframeRef.current?.contentDocument;
      const body = doc?.body;
      const root = doc?.documentElement;
      if (body || root) {
        setHeight(
          Math.max(
            40,
            body?.scrollHeight ?? 0,
            body?.offsetHeight ?? 0,
            root?.scrollHeight ?? 0,
            root?.offsetHeight ?? 0,
          ),
        );
      }
    } catch {
      setHeight(240);
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
      // Keys typed after clicking into a message still reach the app's
      // dispatcher; the frame runs no scripts of its own.
      doc?.addEventListener("keydown", forwardKeydown);
      // A click in the message makes the reader the active pane, or keys
      // forwarded from here would act on the list.
      const interact = () => onInteractRef.current?.();
      doc?.addEventListener("mousedown", interact);
      doc?.addEventListener("focusin", interact);
    } catch {
      // The iframe still renders; fixed fallback height comes from resizeToContent.
    }
    window.setTimeout(resizeToContent, 100);
    window.setTimeout(resizeToContent, 500);
  }

  return (
    <div className="overflow-hidden rounded-md" style={{ backgroundColor: frameBackground }}>
      <iframe
        ref={iframeRef}
        title="HTML message body"
        className="block w-full border-0"
        sandbox={IFRAME_SANDBOX}
        srcDoc={srcDoc}
        style={{
          height,
          backgroundColor: frameBackground,
          colorScheme: effectiveTheme === "dark" ? "dark" : "light",
          opacity: loaded ? 1 : 0,
          transition: "opacity var(--motion-duration-fast) var(--ease-out)",
        }}
        onLoad={handleLoad}
      />
    </div>
  );
}

function forwardKeydown(event: KeyboardEvent) {
  if (event.metaKey || event.ctrlKey) {
    // Leave copy (⌘C) and select-all inside the message alone.
    if (["c", "a", "x"].includes(event.key.toLowerCase())) return;
  }
  const forwarded = new KeyboardEvent("keydown", {
    key: event.key,
    code: event.code,
    shiftKey: event.shiftKey,
    altKey: event.altKey,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    bubbles: true,
    cancelable: true,
  });
  window.dispatchEvent(forwarded);
  if (forwarded.defaultPrevented) event.preventDefault();
}

interface EmailPalette {
  background: string;
  foreground: string;
  muted: string;
  link: string;
  rule: string;
  code: string;
  warning: string;
}

function readPalette(): EmailPalette {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--background", "#071522"),
    foreground: read("--foreground", "#eef5fa"),
    muted: read("--muted-foreground", "#8fa8bc"),
    link: read("--primary", "#57d5ff"),
    rule: read("--border-strong", "#294963"),
    code: read("--muted", "#10263a"),
    warning: read("--warning", "#ffd166"),
  };
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
  palette: EmailPalette,
): string {
  const sanitized = sanitizeHtml(html, {
    allowRemoteImages,
    stripLightBackgrounds: theme === "dark",
    stripDarkTextColors: theme === "dark",
  });
  const style = theme === "dark" ? darkEmailCss(palette) : originalEmailCss;
  return `<!doctype html><html><head><base target="_blank"><meta http-equiv="Content-Security-Policy" content="script-src 'none'"><meta name="color-scheme" content="${theme === "dark" ? "dark" : "light"}"><style>${style}${askMarkCss(theme, palette)}</style></head><body>${sanitized}</body></html>`;
}

/** A standalone copy for "Open original in a new tab" (TUI `O`). */
export function standaloneHtmlDocument(html: string, allowRemoteImages: boolean): string {
  return renderHtmlDocument(html, allowRemoteImages, "original", readPalette());
}

function askMarkCss(theme: EmailHtmlTheme, p: EmailPalette): string {
  const [background, rule] =
    theme === "dark"
      ? [`color-mix(in srgb, ${p.warning} 18%, transparent)`, p.warning]
      : ["#fff1c2", "#b07800"];
  return `mark[${ASK_MARK_ATTRIBUTE}]{background:${background};color:inherit;border-bottom:1px solid ${rule};border-radius:2px;padding:0 .1em;-webkit-box-decoration-break:clone;box-decoration-break:clone}`;
}

const originalEmailCss = `html{color-scheme:light;background:#fff}body{box-sizing:border-box;max-width:860px;margin:0 auto;padding:20px 24px;font:14px/1.5 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#111;background:#fff;overflow-wrap:anywhere}*,*:before,*:after{box-sizing:border-box}table{max-width:100%;border-collapse:collapse}body>table{margin-left:auto;margin-right:auto}img{max-width:100%;height:auto}a[href]{color:#0369a1!important;text-decoration:underline!important;text-underline-offset:2px}a[href]:hover{color:#075985!important}`;

function darkEmailCss(p: EmailPalette): string {
  return `html{color-scheme:dark;background:${p.background}}body{box-sizing:border-box;max-width:860px;margin:0 auto;padding:20px 24px;font:14px/1.6 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:${p.foreground};background:${p.background};overflow-wrap:anywhere}*,*:before,*:after{box-sizing:border-box}table{max-width:100%;border-collapse:collapse}body>table{margin-left:auto;margin-right:auto}a[href]{color:${p.link}!important;text-decoration:underline!important;text-underline-offset:2px}hr{border:0;border-top:1px solid ${p.rule}}pre,code,kbd,samp{background:${p.code};border-radius:4px}pre{padding:12px;white-space:pre-wrap}blockquote{border-left:3px solid ${p.rule};margin-left:0;padding-left:12px;color:${p.muted}}img{max-width:100%;height:auto;filter:brightness(.95)}img[data-original-src]{display:inline-block;min-width:24px;min-height:24px;border:1px dashed ${p.rule};border-radius:4px}`;
}
