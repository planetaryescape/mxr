import DOMPurify from "dompurify";

/** Class of the note that stands in for a blocked remote image. */
export const BLOCKED_IMAGE_CLASS = "mxr-image-blocked";

/**
 * Whether an image source would be fetched from the network. Only inline
 * sources (`data:`, `blob:`, `cid:`) are local; everything else, including
 * protocol-relative `//host/…` and relative paths, counts as remote.
 */
export function isRemoteSrc(src: string): boolean {
  const value = src.trim();
  return value !== "" && !/^(data|blob|cid):/i.test(value);
}

/** The host a remote source would be fetched from, when it names one. */
export function remoteHost(src: string): string | null {
  return parseRemote(src)?.hostname ?? null;
}

const RELATIVE_BASE = "relative.invalid";

/** The URL a remote source names, or null for inline or relative ones. */
function parseRemote(src: string): URL | null {
  if (!isRemoteSrc(src)) return null;
  try {
    const url = new URL(src.trim(), `https://${RELATIVE_BASE}`);
    return url.hostname === RELATIVE_BASE ? null : url;
  } catch {
    return null;
  }
}

interface SanitizeOpts {
  allowRemoteImages?: boolean;
  /**
   * Markup that will sit in the app's own DOM rather than a sandboxed frame:
   * no style, class, id or name attributes (they could restyle or shadow the
   * app), links only to web and mail addresses, images only from the web or
   * inline.
   */
  inline?: boolean;
  stripLightBackgrounds?: boolean;
  stripDarkTextColors?: boolean;
}

export type EmailHtmlCanvas = "dark" | "paper" | "original";
export type EmailHtmlPreference = "auto" | "dark" | "original";

export interface PreparedEmailHtml {
  html: string;
  canvas: EmailHtmlCanvas;
  designed: boolean;
}

interface SanitizedEmailHtml {
  html: string;
  designed: boolean;
}

export function sanitizeHtml(html: string, opts: SanitizeOpts = {}): string {
  return sanitizeHtmlWithDesign(html, opts).html;
}

/**
 * The cleaned article HTML for the Reading reader, which renders it inline
 * (not in a frame) so highlights can be drawn on it. Same tag allowlist, remote
 * image gate and tracker removal as email, minus everything that could affect
 * the page around it.
 */
export function sanitizeReaderHtml(html: string, opts: { allowRemoteImages: boolean }): string {
  return sanitizeHtmlWithDesign(html, { allowRemoteImages: opts.allowRemoteImages, inline: true })
    .html;
}

export function prepareEmailHtml(
  html: string,
  options: { allowRemoteImages: boolean; preference: EmailHtmlPreference; darkScheme: boolean },
): PreparedEmailHtml {
  const sanitized = sanitizeHtmlWithDesign(html, { allowRemoteImages: options.allowRemoteImages });
  const canvas: EmailHtmlCanvas =
    options.preference === "original"
      ? "original"
      : options.preference === "dark"
        ? "dark"
        : !options.darkScheme
          ? "original"
          : sanitized.designed
            ? "paper"
            : "dark";
  return { html: styleForCanvas(sanitized.html, canvas), canvas, designed: sanitized.designed };
}

function sanitizeHtmlWithDesign(html: string, opts: SanitizeOpts = {}): SanitizedEmailHtml {
  const dom = DOMPurify(window);
  let designed = false;
  const beforeSanitizeAttributes = (node: Element) => {
    // Legacy color attributes are deliberately not added to the allowlist;
    // observe their presence only to choose a sender-preserving canvas.
    designed ||= node instanceof window.Element && node.hasAttribute("bgcolor");
  };
  const hook = (node: Element) => {
    if (node.tagName === "A") {
      node.setAttribute("rel", "noopener noreferrer");
      node.setAttribute("target", "_blank");
      if (opts.inline && !isWebOrMailHref(node.getAttribute("href") ?? "")) {
        // A relative or fragment link would navigate the app itself.
        node.removeAttribute("href");
      }
    }
    if (
      opts.inline &&
      node.tagName === "IMG" &&
      !isWebOrInlineImageSrc(node.getAttribute("src") ?? "")
    ) {
      node.remove();
      return;
    }
    if (node.hasAttribute("style")) {
      const style = sanitizeInlineStyle(node.getAttribute("style") ?? "", opts);
      if (style) {
        node.setAttribute("style", style);
      } else {
        node.removeAttribute("style");
      }
    }
    designed ||= hasDesignSignal(node);
    if (node.tagName === "IMG" && isTrackerImage(node)) {
      node.remove();
      return;
    }
    if (
      node.tagName === "IMG" &&
      opts.allowRemoteImages === false &&
      isRemoteSrc(node.getAttribute("src") ?? "")
    ) {
      // A small inline note instead of an empty box the image's size: the
      // sender learns nothing, and the text around it doesn't reflow twice.
      // (Showing images re-renders from the original HTML.)
      const note = node.ownerDocument.createElement("span");
      note.className = BLOCKED_IMAGE_CLASS;
      note.textContent = "Image blocked";
      const alt = node.getAttribute("alt")?.trim();
      if (alt) note.setAttribute("title", alt);
      node.replaceWith(note);
    }
  };
  // Inline markup keeps no classes of its own, but the "Image blocked" note needs its class.
  // DOMPurify visits the note after the hook creates it, so allow that exact value.
  const keepBlockedNoteClass = (
    _node: Element,
    data: { attrName: string; attrValue: string; forceKeepAttr?: boolean },
  ) => {
    if (opts.inline && data.attrName === "class" && data.attrValue === BLOCKED_IMAGE_CLASS) {
      data.forceKeepAttr = true;
    }
  };
  dom.addHook("beforeSanitizeAttributes", beforeSanitizeAttributes);
  dom.addHook("afterSanitizeAttributes", hook);
  dom.addHook("uponSanitizeAttribute", keepBlockedNoteClass);
  let clean: string;
  try {
    const sanitized = dom.sanitize(html, {
      ALLOWED_TAGS: [
        "a",
        "abbr",
        "address",
        "article",
        "aside",
        "b",
        "blockquote",
        "br",
        "caption",
        "cite",
        "code",
        "col",
        "colgroup",
        "dd",
        "del",
        "details",
        "dfn",
        "div",
        "dl",
        "dt",
        "em",
        "figcaption",
        "figure",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hr",
        "i",
        "img",
        "ins",
        "kbd",
        "li",
        "main",
        "mark",
        "nav",
        "ol",
        "p",
        "pre",
        "q",
        "s",
        "samp",
        "section",
        "small",
        "span",
        "strong",
        "sub",
        "summary",
        "sup",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "time",
        "tr",
        "u",
        "ul",
        "var",
        "wbr",
      ],
      ALLOWED_ATTR: opts.inline ? INLINE_ATTRS : FRAME_ATTRS,
      ALLOW_DATA_ATTR: false,
      FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "base", "link", "meta", "form"],
      FORBID_ATTR: ["onerror", "onload", "onclick", "onmouseover", "onfocus", "onblur"],
      ADD_URI_SAFE_ATTR: ["target"],
    });
    // SAFETY: DOMPurify receives a string and returns serialized markup unless Trusted Types are requested.
    clean = sanitized as string;
  } finally {
    dom.removeHook("beforeSanitizeAttributes");
    dom.removeHook("afterSanitizeAttributes");
    dom.removeHook("uponSanitizeAttribute");
  }
  return { html: clean, designed };
}

const INLINE_ATTRS = [
  "href",
  "title",
  "alt",
  "src",
  "width",
  "height",
  "colspan",
  "rowspan",
  "scope",
  "datetime",
  "cite",
  "lang",
  "dir",
];

const FRAME_ATTRS = [...INLINE_ATTRS, "rel", "target", "name", "id", "class", "style"];

function isWebOrMailHref(href: string): boolean {
  return /^(https?:|mailto:)/i.test(href.trim());
}

function isWebOrInlineImageSrc(src: string): boolean {
  return /^(https?:\/\/|data:image\/)/i.test(src.trim());
}

function hasDesignSignal(node: Element): boolean {
  if (node.tagName === "TABLE") {
    const width = Number.parseInt(node.getAttribute("width") ?? "", 10);
    if (Number.isFinite(width) && width >= 500 && width <= 700) return true;
  }
  if (node.tagName === "IMG") {
    const width = Number.parseInt(node.getAttribute("width") ?? "", 10);
    const height = Number.parseInt(node.getAttribute("height") ?? "", 10);
    if ((Number.isFinite(width) && width >= 480) || (Number.isFinite(height) && height >= 240)) {
      return true;
    }
  }
  if (node.hasAttribute("bgcolor")) return true;
  const style = node.getAttribute("style") ?? "";
  const colorDeclaration = style
    .split(";")
    .map((part) => part.trim())
    .find((part) => /^color\s*:/i.test(part));
  const color = colorDeclaration
    ?.slice(colorDeclaration.indexOf(":") + 1)
    .replace(/!important/gi, "")
    .trim();
  if (color) {
    const rgb = readBrowserColor(color)?.rgb;
    if (!rgb || !isNeutralColor(rgb)) return true;
  }
  const background = readInlineBackground(node);
  return background !== null && !isTransparent(background);
}

function isNeutralColor(rgb: [number, number, number]): boolean {
  return Math.max(...rgb) - Math.min(...rgb) <= 12;
}

function readInlineBackground(node: Element): string | null {
  const style = node.getAttribute("style");
  if (!style) return null;
  const declaration = style
    .split(";")
    .map((part) => part.trim())
    .find((part) => /^(background|background-color)\s*:/i.test(part));
  if (!declaration) return null;
  return declaration
    .slice(declaration.indexOf(":") + 1)
    .replace(/!important/gi, "")
    .trim();
}

function styleForCanvas(html: string, canvas: EmailHtmlCanvas): string {
  if (canvas === "original") return html;
  const document = new DOMParser().parseFromString(html, "text/html");
  for (const element of document.body.querySelectorAll<HTMLElement>("[style], [bgcolor]")) {
    let warmedBackground = false;
    if (canvas === "dark") {
      element.removeAttribute("bgcolor");
      element.style.removeProperty("color");
      element.style.removeProperty("background");
      element.style.removeProperty("background-color");
      if (!element.getAttribute("style")?.trim()) element.removeAttribute("style");
      continue;
    }
    if (element.hasAttribute("bgcolor")) {
      const background = element.getAttribute("bgcolor") ?? "";
      if (isLightNeutralBackground(background)) {
        if (canvas === "paper") {
          element.setAttribute("bgcolor", "#f1eee9");
          warmedBackground = true;
        } else element.removeAttribute("bgcolor");
      }
    }
    const style = element.style;
    if (style.backgroundColor && isLightNeutralBackground(style.backgroundColor)) {
      if (canvas === "paper") {
        style.backgroundColor = "#f1eee9";
        warmedBackground = true;
      } else style.removeProperty("background-color");
    }
    if (style.background && isLightNeutralBackground(style.backgroundColor || "")) {
      if (canvas === "paper") {
        style.background = "#f1eee9";
        warmedBackground = true;
      } else style.removeProperty("background");
    }
    const appliedBackground = style.backgroundColor || element.getAttribute("bgcolor");
    if (canvas === "paper" && warmedBackground && style.color && appliedBackground) {
      const readableColor = readableNeutralForeground(style.color, appliedBackground);
      if (readableColor) style.color = readableColor;
    }
  }
  return document.body.innerHTML;
}

function isTransparent(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  if (normalized === "transparent" || normalized === "none") return true;
  return readBrowserColor(normalized)?.alpha === 0;
}

const allowedStyleProperties = new Set([
  "background",
  "background-color",
  "border",
  "border-bottom",
  "border-collapse",
  "border-left",
  "border-radius",
  "border-right",
  "border-top",
  "color",
  "display",
  "font-family",
  "font-size",
  "font-style",
  "font-weight",
  "height",
  "letter-spacing",
  "line-height",
  "margin",
  "margin-bottom",
  "margin-left",
  "margin-right",
  "margin-top",
  "max-width",
  "min-width",
  "padding",
  "padding-bottom",
  "padding-left",
  "padding-right",
  "padding-top",
  "text-align",
  "text-decoration",
  "text-transform",
  "vertical-align",
  "white-space",
  "width",
]);

function sanitizeInlineStyle(style: string, opts: SanitizeOpts = {}): string {
  return style
    .split(";")
    .map((declaration) => declaration.trim())
    .filter(Boolean)
    .flatMap((declaration) => {
      const separator = declaration.indexOf(":");
      if (separator < 1) return [];
      const property = declaration.slice(0, separator).trim().toLowerCase();
      const value = declaration.slice(separator + 1).trim();
      if (!allowedStyleProperties.has(property)) return [];
      if (isUnsafeStyleValue(value)) return [];
      if (
        opts.stripLightBackgrounds &&
        isBackgroundProperty(property) &&
        isLightNeutralBackground(value)
      ) {
        return [];
      }
      if (opts.stripDarkTextColors && property === "color" && isDarkTextColor(value)) {
        return [];
      }
      return [`${property}: ${value}`];
    })
    .join("; ");
}

function isBackgroundProperty(property: string): boolean {
  return property === "background" || property === "background-color";
}

function isLightNeutralBackground(value: string): boolean {
  const normalized = value
    .toLowerCase()
    .replace(/!important/g, "")
    .trim();
  if (normalized === "transparent" || normalized === "none") return false;
  if (/\b(white|whitesmoke|snow|ivory|seashell|linen|oldlace|floralwhite)\b/.test(normalized)) {
    return true;
  }
  return isLightNeutralRgb(readBrowserColor(normalized)?.rgb);
}

function isDarkTextColor(value: string): boolean {
  const rgb = readBrowserColor(value)?.rgb;
  return isDarkRgb(rgb);
}

function readableNeutralForeground(foreground: string, background: string): string | undefined {
  const fg = readBrowserColor(foreground);
  const bg = readBrowserColor(background);
  if (!fg || !bg || bg.alpha < 1) return undefined;
  const [red, green, blue] = fg.rgb;
  if (Math.max(red, green, blue) - Math.min(red, green, blue) > 12) return undefined;
  return contrastRatio(fg.rgb, bg.rgb) < 4.5 ? "#6c6c6c" : undefined;
}

function contrastRatio(
  foreground: [number, number, number],
  background: [number, number, number],
): number {
  const first = relativeLuminance(foreground);
  const second = relativeLuminance(background);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

function relativeLuminance([red, green, blue]: [number, number, number]): number {
  const linear = [red, green, blue].map((value) => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return linear[0]! * 0.2126 + linear[1]! * 0.7152 + linear[2]! * 0.0722;
}

function readBrowserColor(
  value: string,
): { rgb: [number, number, number]; alpha: number } | undefined {
  const probe = document.createElement("span");
  probe.style.position = "fixed";
  probe.style.visibility = "hidden";
  probe.style.backgroundColor = value;
  if (!probe.style.backgroundColor) return undefined;
  document.body.append(probe);
  const normalized = getComputedStyle(probe).backgroundColor;
  probe.remove();
  const channels = normalized.match(/^rgba?\(([^)]+)\)$/)?.[1];
  if (!channels) return undefined;
  const tuple = channels
    .split(/[\s,/]+/)
    .filter(Boolean)
    .map((part) => Number.parseFloat(part));
  if (tuple.length < 3 || tuple.slice(0, 3).some((channel) => !Number.isFinite(channel))) {
    return undefined;
  }
  return {
    rgb: [tuple[0]!, tuple[1]!, tuple[2]!],
    alpha: tuple[3] ?? 1,
  };
}

function isLightNeutralRgb(rgb: [number, number, number] | undefined): boolean {
  if (!rgb) return false;
  const [red, green, blue] = rgb;
  return (
    red >= 235 &&
    green >= 235 &&
    blue >= 235 &&
    Math.max(red, green, blue) - Math.min(red, green, blue) <= 18
  );
}

function isDarkRgb(rgb: [number, number, number] | undefined): boolean {
  if (!rgb) return false;
  const [red, green, blue] = rgb;
  return (red * 0.2126 + green * 0.7152 + blue * 0.0722) / 255 < 0.45;
}

function isUnsafeStyleValue(value: string): boolean {
  return /url\s*\(|expression\s*\(|@import|javascript:|vbscript:|data:|-moz-binding/i.test(value);
}

/** Tracking pixels: tiny images and known open-tracking URLs. Always removed. */
export function isTrackerImage(node: Element): boolean {
  return isTinyImage(node) || isKnownTrackerSrc(node.getAttribute("src") ?? "");
}

function isTinyImage(node: Element): boolean {
  const width = numericAttr(node, "width");
  const height = numericAttr(node, "height");
  return (width !== undefined && width <= 2) || (height !== undefined && height <= 2);
}

function isKnownTrackerSrc(src: string): boolean {
  const url = parseRemote(src);
  if (!url) return false;
  const host = url.hostname.toLowerCase();
  const path = url.pathname.toLowerCase();
  return (
    host === "mailtrack.io" ||
    host === "track.customer.io" ||
    host.startsWith("email.mg.") ||
    (host === "sendgrid.net" && path.startsWith("/wf/open")) ||
    (host === "mandrillapp.com" && path.startsWith("/track"))
  );
}

function numericAttr(node: Element, attr: string): number | undefined {
  const value = node.getAttribute(attr);
  if (!value) return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : undefined;
}
