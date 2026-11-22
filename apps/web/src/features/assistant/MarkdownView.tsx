import { createElement, Fragment, useMemo, type ReactNode } from "react";

import { APP_LINK_ATTRIBUTE, APP_LINK_ID_ATTRIBUTE, renderMarkdown } from "./markdown";
import { Avatar } from "@/components/Avatar";
import { useThreadModesMap } from "@/features/modes/membership";
import { LinkHoverCard } from "./LinkHoverCard";
import { threadGlyph, type ModesByThread } from "./threadGlyph";

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
  appLinks?: boolean;
  className?: string;
}) {
  const body = useMemo(
    () => new DOMParser().parseFromString(renderMarkdown(text, { appLinks }), "text/html").body,
    [text, appLinks],
  );
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

function toLink(
  node: HTMLElement,
  key: string,
  children: ReactNode[],
  modes: ModesByThread,
): ReactNode {
  const href = node.getAttribute("href") ?? "";
  const kind = node.getAttribute(APP_LINK_ATTRIBUTE);
  if (kind === "person" || kind === "thread") {
    const id = node.getAttribute(APP_LINK_ID_ATTRIBUTE);
    const Glyph = threadGlyph(id, modes);
    const name = node.textContent ?? "";
    const chip = (
      <a
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
    if (!id) return <Fragment key={key}>{chip}</Fragment>;
    return (
      <LinkHoverCard
        key={key}
        link={
          kind === "person" ? { kind: "person", person_id: id } : { kind: "thread", thread_id: id }
        }
        title={name}
        modes={modes}
      >
        {chip}
      </LinkHoverCard>
    );
  }
  return (
    <a key={key} href={href} target="_blank" rel="noopener noreferrer nofollow">
      {children}
    </a>
  );
}
