import type { ComposeSessionKind } from "../../shared/types";

export function stringField(value: unknown) {
  return typeof value === "string" ? value : null;
}

export function renderReaderBody(body: string, signatureExpanded: boolean) {
  if (signatureExpanded) {
    return body;
  }
  for (const marker of ["\n-- \n", "\n--\n", "\nSent from my"]) {
    const index = body.indexOf(marker);
    if (index >= 0) {
      return body.slice(0, index).trimEnd();
    }
  }
  return body;
}

export function renderReaderParagraphs(body: string) {
  return body
    .split(/\n\s*\n/g)
    .map((paragraph) => paragraph.trim())
    .filter(Boolean);
}

export function formatJson(value: unknown) {
  if (value == null) {
    return "No data";
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function composeKindLabel(kind: ComposeSessionKind) {
  return kind.replaceAll("_", " ");
}

export function composeTitle(kind: ComposeSessionKind) {
  switch (kind) {
    case "reply":
      return "Reply";
    case "reply_all":
      return "Reply all";
    case "forward":
      return "Forward";
    default:
      return "New message";
  }
}

export function escapeHtml(value: string) {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
