import { Bookmark, Check, ExternalLink, MailX } from "lucide-react";
import { memo } from "react";

import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

import type { ReadingItem, ReadingLink } from "./api";
import { domainLabel, minutesLabel, shownLinks } from "./readingView";

export interface CardHandlers {
  onFocus: (key: string) => void;
  onRead: (key: string) => void;
  onLater: (key: string) => void;
  onLetGo: (item: ReadingItem) => void;
  onUnsubscribe: (item: ReadingItem) => void;
  onToggleLinks: (key: string) => void;
}

/**
 * One item of the edition: what a scan needs, in the order eyes take it
 * (headline, then source and minutes, then the standfirst). A lead item
 * gets a card with its keys; the rest are rows. A digest lists its first
 * links inline. Nothing is bold for being new and nothing is counted.
 */
export const ReadingItemCard = memo(function ReadingItemCard({
  item,
  index,
  linkIndex,
  focusedKey,
  expanded,
  fading,
  shelf,
  handlers,
}: {
  item: ReadingItem;
  /** Cursor position of the item itself. */
  index: number;
  /** Cursor position of each shown link, by key. */
  linkIndex: ReadonlyMap<string, number>;
  focusedKey: string | null;
  expanded: boolean;
  fading: boolean;
  /** On the Later shelf: no fade, and the "still want it" question. */
  shelf?: boolean;
  handlers: CardHandlers;
}) {
  const focused = focusedKey === item.item_key;
  const lead = Boolean(item.lead) && !shelf;
  const { shown, more } = shownLinks(item.links ?? [], expanded);
  const meta = [item.source, minutesLabel(item.minutes), item.engagement].filter(Boolean);
  return (
    <article
      data-testid="reading-item"
      data-index={index}
      data-key={item.item_key}
      data-thread={item.thread_id}
      data-sender={item.sender_email}
      data-lead={lead ? "true" : undefined}
      aria-current={focused ? "true" : undefined}
      aria-label={`${item.source}: ${item.title}`}
      onMouseDown={() => handlers.onFocus(item.item_key)}
      className={cn(
        "relative mx-4 my-1.5 rounded-md px-4 py-3 sm:mx-5",
        lead ? "border border-border bg-surface py-4" : "border border-transparent",
        focused && "border-border-strong",
        fading && "reading-fading",
      )}
    >
      {focused ? (
        <span
          aria-hidden
          className="absolute -left-0.5 top-3 h-8 w-[2px] rounded-full bg-primary"
        />
      ) : null}
      <h3
        className={cn(
          "text-balance font-semibold leading-snug tracking-tight text-foreground",
          lead ? "text-[19px]" : "text-[15.5px]",
        )}
      >
        <a
          href={`/reading/item/${encodeURIComponent(item.item_key)}`}
          onClick={(event) => {
            event.preventDefault();
            handlers.onRead(item.item_key);
          }}
          className="hover:underline hover:decoration-border-strong hover:underline-offset-4"
        >
          {item.title}
        </a>
      </h3>
      <p className="mt-0.5 text-[12.5px] text-muted-foreground" data-testid="reading-meta">
        {meta.join(" · ")}
        {item.on_later ? (
          <span className="ml-2 inline-flex items-center gap-1 text-foreground/80">
            <Bookmark aria-hidden className="size-3" /> On Later
          </span>
        ) : null}
      </p>
      {item.standfirst ? (
        <p
          className={cn(
            "mt-1.5 max-w-[66ch] text-pretty text-[13.5px] leading-6 text-foreground/85",
            !lead && "line-clamp-2",
          )}
        >
          {item.standfirst}
        </p>
      ) : null}
      {shown.length > 0 ? (
        <ul className="mt-2 grid gap-0.5" aria-label={`Links in ${item.title}`}>
          {shown.map((link) => (
            <LinkRow
              key={link.item_key}
              link={link}
              index={linkIndex.get(link.item_key) ?? -1}
              focused={focusedKey === link.item_key}
              handlers={handlers}
            />
          ))}
          {more > 0 ? (
            <li>
              <button
                type="button"
                onClick={() => handlers.onToggleLinks(item.item_key)}
                className="pl-4 text-[12.5px] text-muted-foreground hover:text-foreground"
              >
                + {more} more
              </button>
            </li>
          ) : null}
        </ul>
      ) : null}
      {item.unsubscribe_offer && !shelf ? (
        <p data-testid="unsubscribe-offer" className="mt-2 text-[12.5px] text-muted-foreground">
          {item.unsubscribe_offer}.{" "}
          <button
            type="button"
            onClick={() => handlers.onUnsubscribe(item)}
            className="inline-flex items-center gap-1 text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
          >
            <MailX aria-hidden className="size-3" /> Unsubscribe?
          </button>{" "}
          <KeyChip className="h-4 px-1">D</KeyChip>
        </p>
      ) : null}
      <p data-testid="why-here" className="mt-1.5 text-[12px] text-muted-foreground">
        {item.why}
      </p>
      {lead ? (
        <p className="mt-2 hidden flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-muted-foreground sm:flex">
          <button
            type="button"
            onClick={() => handlers.onRead(item.item_key)}
            className="inline-flex items-center gap-1 hover:text-foreground"
          >
            <KeyChip>↵</KeyChip> read
          </button>
          <button
            type="button"
            onClick={() => handlers.onLater(item.item_key)}
            className="inline-flex items-center gap-1 hover:text-foreground"
          >
            <KeyChip>b</KeyChip> later
          </button>
          <button
            type="button"
            onClick={() => handlers.onLetGo(item)}
            className="inline-flex items-center gap-1 hover:text-foreground"
          >
            <Check aria-hidden className="size-3" /> <KeyChip>e</KeyChip> let go
          </button>
        </p>
      ) : null}
    </article>
  );
});

function LinkRow({
  link,
  index,
  focused,
  handlers,
}: {
  link: ReadingLink;
  index: number;
  focused: boolean;
  handlers: CardHandlers;
}) {
  return (
    <li
      data-testid="reading-link"
      data-index={index}
      data-key={link.item_key}
      aria-current={focused ? "true" : undefined}
      onMouseDown={(event) => {
        event.stopPropagation();
        handlers.onFocus(link.item_key);
      }}
      className={cn(
        "grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-3 rounded px-2 py-0.5 text-[13.5px]",
        focused && "bg-accent",
      )}
    >
      <a
        href={`/reading/item/${encodeURIComponent(link.item_key)}`}
        onClick={(event) => {
          event.preventDefault();
          handlers.onRead(link.item_key);
        }}
        className="min-w-0 truncate text-foreground/90 hover:underline hover:underline-offset-4"
        title={link.blurb ?? link.title}
      >
        <span aria-hidden className="mr-1.5 text-muted-foreground">
          ›
        </span>
        {link.title}
      </a>
      <span className="inline-flex items-center gap-1.5 font-mono text-2xs text-muted-foreground">
        {link.on_later ? <Bookmark aria-label="On Later" className="size-3" /> : null}
        {link.article_cached ? (
          <ExternalLink aria-label="Article saved" className="size-3" />
        ) : null}
        <span className="max-w-[12rem] truncate">{domainLabel(link.domain, link.tracked)}</span>
      </span>
    </li>
  );
}
