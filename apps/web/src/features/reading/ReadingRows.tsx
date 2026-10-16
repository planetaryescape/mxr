import { Bookmark, ChevronRight, ExternalLink, Newspaper } from "lucide-react";
import { memo, type ReactNode } from "react";

import { ModeMarks } from "@/components/ModeMark";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { otherModes, type ThreadModes } from "@/features/modes/membership";
import { cn } from "@/lib/utils";

import type { ReadingItem, ReadingLink } from "./api";
import { domainLabel, minutesLabel } from "./readingView";

export interface RowHandlers {
  /** A pointer chose the row (the lamp fades to it). */
  onPoint: (key: string) => void;
  /** Keyboard focus reached the row's link. */
  onFocus: (key: string) => void;
  onRead: (key: string) => void;
  onLater: (key: string) => void;
  onLetGo: (item: ReadingItem) => void;
  onUnsubscribe: (item: ReadingItem) => void;
  onToggleLinks: (key: string) => void;
}

/** The row's one interactive element: its text is a real link, as Now's rows are. */
function readLink(key: string, handlers: RowHandlers, label: string) {
  return (content: ReactNode, className: string) => (
    <a
      href={`/reading/item/${encodeURIComponent(key)}`}
      aria-label={label}
      onFocus={() => handlers.onFocus(key)}
      onClick={(event) => {
        event.preventDefault();
        handlers.onRead(key);
      }}
      className={cn(className, "focus-visible:ring-2 focus-visible:ring-signal")}
    >
      {content}
    </a>
  );
}

/**
 * One issue of the edition as a `Row` (D129): the headline, its standfirst on
 * the one secondary line, and in the trail the source and minutes. The keys
 * show on the lit row only; the reason goes to the status bar.
 */
export const ReadingIssueRow = memo(function ReadingIssueRow({
  item,
  index,
  lit,
  via,
  fading,
  shelf,
  modes,
  handlers,
}: {
  item: ReadingItem;
  /** Cursor position of the item itself. */
  index: number;
  lit: boolean;
  via: LampVia;
  fading: boolean;
  /** On the Later shelf: no fade. */
  shelf?: boolean;
  modes?: ThreadModes;
  handlers: RowHandlers;
}) {
  const meta = [item.source, minutesLabel(item.minutes)].filter(Boolean).join(" · ");
  const canUnsubscribe = Boolean(item.unsubscribe_offer) && !shelf;
  return (
    <Row
      data-testid="reading-item"
      data-index={index}
      data-key={item.item_key}
      data-thread={item.thread_id}
      data-sender={item.sender_email}
      data-focused={lit ? "true" : undefined}
      aria-current={lit ? "true" : undefined}
      lit={lit}
      via={via}
      land={[item.thread_id, item.message_id]}
      className={cn(fading && !shelf && "reading-fading")}
      onMouseDown={() => handlers.onPoint(item.item_key)}
      lead={<Newspaper aria-hidden className="size-4 shrink-0 text-text-3" strokeWidth={1.75} />}
      primary={item.title}
      secondary={item.standfirst || item.engagement || undefined}
      renderMain={readLink(item.item_key, handlers, `${item.source}: ${item.title}`)}
      trail={
        <>
          {item.on_later ? (
            <Bookmark aria-label="On Later" className="size-3.5" strokeWidth={1.75} />
          ) : null}
          <ModeMarks modes={otherModes(modes, "reading")} />
          <span data-testid="reading-meta" className="hidden whitespace-nowrap sm:inline">
            {meta}
          </span>
        </>
      }
      verbs={
        <>
          <RowVerb
            keys="Enter"
            label="read"
            testId="reading-read"
            onClick={() => handlers.onRead(item.item_key)}
          />
          <RowVerb
            keys="b"
            label="later"
            testId="reading-later"
            onClick={() => handlers.onLater(item.item_key)}
          />
          <RowVerb
            keys="e"
            label="let go"
            testId="reading-let-go"
            onClick={() => handlers.onLetGo(item)}
          />
          {canUnsubscribe ? (
            <RowVerb
              keys="D"
              label="unsubscribe"
              testId="reading-row-unsubscribe"
              onClick={() => handlers.onUnsubscribe(item)}
            />
          ) : null}
        </>
      }
    />
  );
});

/** A digest's link, as a row indented under its issue. */
export const ReadingLinkRow = memo(function ReadingLinkRow({
  link,
  index,
  lit,
  via,
  handlers,
  hint,
}: {
  link: ReadingLink;
  index: number;
  lit: boolean;
  via: LampVia;
  handlers: RowHandlers;
  hint?: ReactNode;
}) {
  return (
    <Row
      data-testid="reading-link"
      data-index={index}
      data-key={link.item_key}
      data-focused={lit ? "true" : undefined}
      aria-current={lit ? "true" : undefined}
      lit={lit}
      via={via}
      className="ml-6"
      hint={hint}
      onMouseDown={(event) => {
        event.stopPropagation();
        handlers.onPoint(link.item_key);
      }}
      lead={<ChevronRight aria-hidden className="size-4 shrink-0 text-text-3" strokeWidth={1.75} />}
      primary={link.title}
      secondary={link.blurb || undefined}
      renderMain={readLink(link.item_key, handlers, link.title)}
      trail={
        <>
          {link.on_later ? (
            <Bookmark aria-label="On Later" className="size-3.5" strokeWidth={1.75} />
          ) : null}
          {link.article_cached ? (
            <ExternalLink aria-label="Article saved" className="size-3.5" strokeWidth={1.75} />
          ) : null}
          <span className="hidden max-w-[12rem] truncate sm:inline">
            {domainLabel(link.domain, link.tracked)}
          </span>
        </>
      }
      verbs={
        <>
          <RowVerb keys="Enter" label="read" onClick={() => handlers.onRead(link.item_key)} />
          <RowVerb keys="b" label="later" onClick={() => handlers.onLater(link.item_key)} />
        </>
      }
    />
  );
});
