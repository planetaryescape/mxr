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

import { APP_LINK_ATTRIBUTE, APP_LINK_ID_ATTRIBUTE, renderMarkdown } from "./markdown";
import { Avatar } from "@/components/Avatar";
import { MODE_GLYPHS } from "@/components/ModeMark";
import { useThreadModesMap, type ThreadModes } from "@/features/modes/membership";

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

type ModesByThread = ReadonlyMap<string, ThreadModes> | undefined;

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
  const body = useMemo(
    () => new DOMParser().parseFromString(renderMarkdown(text, { appLinks }), "text/html").body,
    [text, appLinks],
  );
  // One request for every email chip in the reply, not one per chip.
  const threadIds = useMemo(
    () =>
      [...body.querySelectorAll(`a[${APP_LINK_ATTRIBUTE}="thread"]`)].flatMap((link) => {
        const id = link.getAttribute(APP_LINK_ID_ATTRIBUTE);
        return id ? [id] : [];
      }),
    [body],
  );
  const modes = useThreadModesMap(threadIds).data;
  const content = useMemo(
    () => [...body.childNodes].map((node, index) => toNode(node, String(index), modes)),
    [body, modes],
  );
  return <div className={className}>{content}</div>;
}

function toNode(node: ChildNode, key: string, modes: ModesByThread): ReactNode {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent;
  if (!(node instanceof HTMLElement)) return null;
  const tag = node.tagName.toLowerCase();
  const children = [...node.childNodes].map((child, index) =>
    toNode(child, `${key}.${index}`, modes),
  );
  if (tag === "a") return toLink(node, key, children, modes);
  if (!TAGS.has(tag)) return children;
  return VOID.has(tag) ? createElement(tag, { key }) : createElement(tag, { key }, children);
}

/**
 * The glyph of where an email lives: its first mode in rail order, else
 * Inbox when only the provider's inbox holds it. Mail while the lookup is
 * out or the thread is unknown (docs/blueprint/27 "Iconography").
 */
function threadGlyph(threadId: string | null, modes: ModesByThread) {
  const held = threadId ? modes?.get(threadId) : undefined;
  const mode = held?.modes[0]?.mode ?? (held?.in_inbox ? "inbox" : null);
  return mode ? MODE_GLYPHS[mode] : Mail;
}

function toLink(
  node: HTMLElement,
  key: string,
  children: ReactNode[],
  modes: ModesByThread,
): ReactNode {
  const href = node.getAttribute("href") ?? "";
  const kind = node.getAttribute(APP_LINK_ATTRIBUTE);
  if (kind === "person" || kind === "thread") {
    const Glyph = threadGlyph(node.getAttribute(APP_LINK_ID_ATTRIBUTE), modes);
    const name = node.textContent ?? "";
    return (
      <a
        key={key}
        href={href}
        {...{ [APP_LINK_ATTRIBUTE]: kind }}
        className="assistant-chip type-body inline-flex max-w-full items-center gap-1 rounded-sm border border-rule px-1 align-baseline text-signal outline-none hover:bg-signal-soft focus-visible:border-signal"
      >
        {kind === "person" ? (
          <Avatar name={name} size="xs" />
        ) : (
          <Glyph aria-hidden className="size-3.5 shrink-0" strokeWidth={1.75} />
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
