import TurndownService from "turndown";
import {
  FOLD_QUOTE_MIN_LINES,
  normalizeSegments,
  splitMessageText,
} from "../../../web/src/features/thread/textSegments";
import type { MessageBody } from "../bridge/mail";
import { escapeLine } from "./markdown";

// Mail bodies as Raycast markdown. Plain text goes through the web reader's
// own split, so what folds (long quotes, signatures) folds the same way in
// both clients; HTML-only mail is converted, without images, since Raycast
// would fetch remote ones and so tell senders the mail was opened.

const turndown = new TurndownService({ headingStyle: "atx", codeBlockStyle: "fenced" });
turndown.remove(["style", "script", "head", "title"]);
// `remove` loses to turndown's own image rule, so images get a rule of
// their own: the alt text, never the remote address (tracking pixels).
turndown.addRule("no-remote-images", {
  filter: "img",
  replacement: (_content, node) => {
    // The extension builds without the DOM lib; turndown's node is an element.
    const alt = (node as unknown as { getAttribute(name: string): string | null })
      .getAttribute("alt")
      ?.trim();
    return alt ? `[${alt}]` : "";
  },
});

/**
 * Sender-controlled header text (subject, names, file names) on one line,
 * escaped like body text so it can never become an image, link or heading.
 */
export function inline(text: string): string {
  return escapeLine(text.replace(/\s+/g, " ").trim());
}

/**
 * Plain text as markdown paragraphs, line breaks kept inside each. Blank
 * lines must stay paragraph breaks: Raycast's arrow keys scroll a detail
 * view a block at a time, so one block per message jumps end to end.
 */
function paragraphs(text: string): string {
  return text
    .split(/\n\s*\n/)
    .map((paragraph) =>
      paragraph
        .split("\n")
        .map((line) => escapeLine(line.trimEnd()))
        .join("  \n"),
    )
    .filter((paragraph) => paragraph.trim())
    .join("\n\n");
}

/** Runs of `<br>` from HTML mail are paragraph breaks too, for the same reason. */
function splitBreaks(markdown: string): string {
  return markdown.replace(/( {2}\n[ \t]*){2,}/g, "\n\n").replace(/\n{3,}/g, "\n\n");
}

function plainText(text: string): string {
  const parts: string[] = [];
  let folded = false;
  for (const segment of normalizeSegments(splitMessageText(text))) {
    if (segment.kind === "text") {
      parts.push(paragraphs(segment.text.trim()));
    } else if (segment.kind === "quote" && segment.lines < FOLD_QUOTE_MIN_LINES) {
      const quoted = segment.text
        .split("\n")
        .map((line) => `> ${escapeLine(line.replace(/^\s*>\s?/, ""))}`)
        .join("  \n");
      parts.push(quoted);
    } else {
      folded = true;
    }
  }
  if (folded) parts.push("*Quoted text and signature hidden.*");
  return parts.filter((part) => part.trim()).join("\n\n");
}

/** Some senders (help desks) put their HTML in the plain-text part too. */
const MARKUP = /<(br|p|div|h[1-6]|strong|table)\b[^>]*>/i;

/** One message's body as markdown, or a note when there is none. */
export function bodyMarkdown(body: MessageBody | undefined): string {
  const plain = body?.text_plain?.trim();
  if (plain && !(MARKUP.test(plain) && body?.text_html?.trim())) return plainText(plain);
  if (body?.text_html?.trim()) return splitBreaks(turndown.turndown(body.text_html)).trim();
  return "*No text in this message.*";
}
