/*
 * A reply's markdown as React. The text is model text built from other
 * people's mail, so it goes through the sanitiser first (markdown.ts), and
 * the result is walked into elements here rather than set as HTML: only a
 * short list of tags and the `href` and `title` the sanitiser allowed come
 * through, text is escaped by React, and a link to an email or a person
 * becomes a chip drawn from its data (docs/blueprint/29 "The copy").
 */

import { Mail } from "lucide-react";
import { createElement, useMemo, type ReactNode } from "react";

import { APP_LINK_ATTRIBUTE, renderMarkdown } from "./markdown";
import { Avatar } from "@/components/Avatar";

const TAGS = new Set([
  "p",
  "br",
  "strong",
  "em",
  "code",
  "pre",
  "ul",
  "ol",
  "li",
  "blockquote",
  "h1",
  "h2",
  "h3",
  "h4",
  "hr",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
  "del",
]);
const VOID = new Set(["br", "hr"]);

export function MarkdownView({
  text,
  appLinks = false,
  className,
}: {
  text: string;
  /** Only for a stored reply: the daemon has checked its links against what its tools returned. */
  appLinks?: boolean;
  className?: string;
}) {
  const content = useMemo(() => {
    const html = renderMarkdown(text, { appLinks });
    const body = new DOMParser().parseFromString(html, "text/html").body;
    return [...body.childNodes].map((node, index) => toNode(node, String(index)));
  }, [text, appLinks]);
  return <div className={className}>{content}</div>;
}

function toNode(node: ChildNode, key: string): ReactNode {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent;
  if (!(node instanceof HTMLElement)) return null;
  const tag = node.tagName.toLowerCase();
  const children = [...node.childNodes].map((child, index) => toNode(child, `${key}.${index}`));
  if (tag === "a") return toLink(node, key, children);
  if (!TAGS.has(tag)) return children;
  return VOID.has(tag) ? createElement(tag, { key }) : createElement(tag, { key }, children);
}

function toLink(node: HTMLElement, key: string, children: ReactNode[]): ReactNode {
  const href = node.getAttribute("href") ?? "";
  const kind = node.getAttribute(APP_LINK_ATTRIBUTE);
  if (kind === "person" || kind === "thread") {
    const name = node.textContent ?? "";
    return (
      <a
        key={key}
        href={href}
        {...{ [APP_LINK_ATTRIBUTE]: kind }}
        className="assistant-chip type-body inline-flex max-w-full items-center gap-1 rounded-sm border border-rule-soft px-1 align-baseline text-text-1 outline-none hover:bg-ink-lamp focus-visible:border-signal"
      >
        {kind === "person" ? (
          <Avatar name={name} size="xs" />
        ) : (
          <Mail aria-hidden className="size-3.5 shrink-0 text-text-3" strokeWidth={1.75} />
        )}
        <span className="truncate">{children}</span>
      </a>
    );
  }
  return (
    <a key={key} href={href} target="_blank" rel="noopener noreferrer nofollow">
      {children}
    </a>
  );
}
