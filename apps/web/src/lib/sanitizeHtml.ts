import DOMPurify from "dompurify";

interface SanitizeOpts {
  allowRemoteImages?: boolean;
  stripLightBackgrounds?: boolean;
  stripDarkTextColors?: boolean;
}

export function sanitizeHtml(html: string, opts: SanitizeOpts = {}): string {
  const dom = DOMPurify(window);
  dom.addHook("afterSanitizeAttributes", (node: Element) => {
    if (node.tagName === "A") {
      node.setAttribute("rel", "noopener noreferrer");
      node.setAttribute("target", "_blank");
    }
    if (node.hasAttribute("style")) {
      const style = sanitizeInlineStyle(node.getAttribute("style") ?? "", opts);
      if (style) {
        node.setAttribute("style", style);
      } else {
        node.removeAttribute("style");
      }
    }
    if (node.tagName === "IMG" && isTrackerImage(node)) {
      node.remove();
      return;
    }
    if (node.tagName === "IMG" && opts.allowRemoteImages === false) {
      const src = node.getAttribute("src") ?? "";
      if (/^https?:\/\//i.test(src)) {
        node.setAttribute("data-original-src", src);
        node.removeAttribute("src");
        node.setAttribute("alt", node.getAttribute("alt") ?? "Remote image (blocked)");
      }
    }
  });
  return dom.sanitize(html, {
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
    ALLOWED_ATTR: [
      "href",
      "title",
      "alt",
      "src",
      "width",
      "height",
      "rel",
      "target",
      "name",
      "colspan",
      "rowspan",
      "scope",
      "datetime",
      "cite",
      "lang",
      "dir",
      "id",
      "class",
      "style",
    ],
    ALLOW_DATA_ATTR: false,
    FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "base", "link", "meta", "form"],
    FORBID_ATTR: ["onerror", "onload", "onclick", "onmouseover", "onfocus", "onblur"],
    ADD_URI_SAFE_ATTR: ["target"],
  }) as unknown as string;
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
        isLightBackground(value)
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

function isLightBackground(value: string): boolean {
  const normalized = value
    .toLowerCase()
    .replace(/!important/g, "")
    .trim();
  if (normalized === "transparent" || normalized === "none") return false;
  if (/\b(white|whitesmoke|snow|ivory|seashell|linen|oldlace|floralwhite)\b/.test(normalized)) {
    return true;
  }
  const hex = normalized.match(/#([0-9a-f]{3}|[0-9a-f]{6})\b/);
  const hexValue = hex?.[1];
  if (hexValue) return isLightNeutralRgb(hexToRgb(hexValue));
  const rgb = normalized.match(/rgba?\(([^)]+)\)/);
  const rgbValue = rgb?.[1];
  if (rgbValue) return isLightNeutralRgb(rgbToTuple(rgbValue));
  return false;
}

function isDarkTextColor(value: string): boolean {
  const normalized = value
    .toLowerCase()
    .replace(/!important/g, "")
    .trim();
  if (/\b(black|navy|midnightblue|darkslategray|darkslategrey)\b/.test(normalized)) return true;
  const hex = normalized.match(/#([0-9a-f]{3}|[0-9a-f]{6})\b/);
  const hexValue = hex?.[1];
  if (hexValue) return isDarkRgb(hexToRgb(hexValue));
  const rgb = normalized.match(/rgba?\(([^)]+)\)/);
  const rgbValue = rgb?.[1];
  if (rgbValue) return isDarkRgb(rgbToTuple(rgbValue));
  const hsl = normalized.match(/hsla?\([^,]+,\s*[^,]+,\s*([0-9.]+)%/);
  const lightness = hsl?.[1];
  if (lightness) return Number.parseFloat(lightness) < 45;
  return false;
}

function hexToRgb(hex: string): [number, number, number] | undefined {
  const expanded =
    hex.length === 3
      ? hex
          .split("")
          .map((char) => `${char}${char}`)
          .join("")
      : hex;
  const parsed = Number.parseInt(expanded, 16);
  if (!Number.isFinite(parsed)) return undefined;
  return [(parsed >> 16) & 255, (parsed >> 8) & 255, parsed & 255];
}

function rgbToTuple(value: string): [number, number, number] | undefined {
  const channels = value
    .split(",")
    .slice(0, 3)
    .map((part) => Number.parseFloat(part.trim()));
  if (channels.length !== 3 || channels.some((channel) => !Number.isFinite(channel))) {
    return undefined;
  }
  const [red, green, blue] = channels;
  if (red === undefined || green === undefined || blue === undefined) return undefined;
  return [red, green, blue];
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

function isTrackerImage(node: Element): boolean {
  return isTinyImage(node) || isKnownTrackerSrc(node.getAttribute("src") ?? "");
}

function isTinyImage(node: Element): boolean {
  const width = numericAttr(node, "width");
  const height = numericAttr(node, "height");
  return (width !== undefined && width <= 2) || (height !== undefined && height <= 2);
}

function isKnownTrackerSrc(src: string): boolean {
  try {
    const url = new URL(src);
    const host = url.hostname.toLowerCase();
    const path = url.pathname.toLowerCase();
    return (
      host === "mailtrack.io" ||
      host === "track.customer.io" ||
      host.startsWith("email.mg.") ||
      (host === "sendgrid.net" && path.startsWith("/wf/open")) ||
      (host === "mandrillapp.com" && path.startsWith("/track"))
    );
  } catch {
    return false;
  }
}

function numericAttr(node: Element, attr: string): number | undefined {
  const value = node.getAttribute(attr);
  if (!value) return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : undefined;
}
