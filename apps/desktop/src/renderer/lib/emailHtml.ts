import DOMPurify from "dompurify";

export type SanitizeEmailHtmlOptions = {
  allowRemoteContent?: boolean;
};

type EmailDocumentOptions = SanitizeEmailHtmlOptions & {
  title: string;
  html: string;
  css?: string;
};

const ACTIVE_CONTENT_TAGS = [
  "script",
  "noscript",
  "iframe",
  "object",
  "embed",
  "applet",
  "form",
  "input",
  "button",
  "textarea",
  "select",
  "option",
  "link",
  "meta",
  "base",
];

const URL_ATTRIBUTES = [
  "action",
  "background",
  "formaction",
  "href",
  "poster",
  "src",
  "xlink:href",
];

export function sanitizeEmailHtml(html: string, options: SanitizeEmailHtmlOptions = {}): string {
  const allowRemoteContent = options.allowRemoteContent === true;
  const sanitized = DOMPurify.sanitize(html, {
    ALLOW_DATA_ATTR: false,
    FORBID_ATTR: ["srcdoc"],
    FORBID_TAGS: ACTIVE_CONTENT_TAGS,
    USE_PROFILES: { html: true },
    WHOLE_DOCUMENT: /<(?:html|body|head)(?:\s|>)/i.test(html),
  });

  const doc = new DOMParser().parseFromString(sanitized, "text/html");
  scrubUrls(doc.body, allowRemoteContent);
  return doc.body.innerHTML;
}

export function buildSanitizedEmailDocument(options: EmailDocumentOptions): string {
  const allowRemoteContent = options.allowRemoteContent === true;
  const bodyHtml = sanitizeEmailHtml(options.html, { allowRemoteContent });
  const styleTag = options.css ? `<style>${options.css}</style>` : "";

  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${escapeHtmlAttribute(
    buildEmailCsp(allowRemoteContent),
  )}"><meta name="referrer" content="no-referrer"><title>${escapeHtml(
    options.title,
  )}</title>${styleTag}</head><body>${bodyHtml}</body></html>`;
}

export function buildPlainTextEmailDocument(title: string, text: string): string {
  return buildSanitizedEmailDocument({
    title,
    html: `<pre>${escapeHtml(text)}</pre>`,
    css: "pre { white-space: pre-wrap; overflow-wrap: anywhere; }",
  });
}

function scrubUrls(root: HTMLElement, allowRemoteContent: boolean) {
  for (const element of Array.from(root.querySelectorAll<HTMLElement>("*"))) {
    for (const attr of URL_ATTRIBUTES) {
      const value = element.getAttribute(attr);
      if (value === null) {
        continue;
      }
      if (shouldStripUrl(value, allowRemoteContent)) {
        element.removeAttribute(attr);
      }
    }

    if (element.hasAttribute("srcset")) {
      if (!allowRemoteContent || containsForbiddenUrl(element.getAttribute("srcset") ?? "")) {
        element.removeAttribute("srcset");
      }
    }

    const style = element.getAttribute("style");
    if (style !== null && shouldStripStyle(style, allowRemoteContent)) {
      element.removeAttribute("style");
    }
  }
}

function shouldStripUrl(value: string, allowRemoteContent: boolean): boolean {
  const trimmed = value.trim();
  if (trimmed === "" || trimmed.startsWith("#")) {
    return false;
  }

  if (allowRemoteContent) {
    return containsForbiddenUrl(trimmed);
  }

  return !isOfflineUrl(trimmed);
}

function shouldStripStyle(value: string, allowRemoteContent: boolean): boolean {
  if (containsForbiddenUrl(value)) {
    return true;
  }
  if (allowRemoteContent) {
    return false;
  }
  return /(?:@import|url\s*\()/i.test(value);
}

function isOfflineUrl(value: string): boolean {
  return /^(?:cid:|data:image\/|#)/i.test(value);
}

function containsForbiddenUrl(value: string): boolean {
  return /(?:javascript:|file:|vbscript:)/i.test(value);
}

function buildEmailCsp(allowRemoteContent: boolean): string {
  const imageSources = allowRemoteContent ? "data: cid: http: https:" : "data: cid:";
  return [
    "default-src 'none'",
    "script-src 'none'",
    "connect-src 'none'",
    `img-src ${imageSources}`,
    "style-src 'unsafe-inline'",
    "font-src data:",
    "media-src 'none'",
    "frame-src 'none'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
  ].join("; ");
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function escapeHtmlAttribute(value: string): string {
  return escapeHtml(value).replace(/`/g, "&#96;");
}
