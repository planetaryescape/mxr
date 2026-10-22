/*
 * The preview an email or person chip (or a reply row's link) shows on
 * hover or keyboard focus, only inside the assistant panel (docs/blueprint/29
 * "The copy"). The trigger stays the plain link, so a click or Enter still
 * navigates; the card has no buttons. It opens at once with what the chip
 * knows (a subject or a name) and fills in as the data arrives, with no
 * spinner. Touch never opens it, so a tap just follows the link. Everything
 * here is mail or model text and is only ever set as text.
 */

import { Paperclip } from "lucide-react";
import { useId, useRef, type ReactNode } from "react";

import { Avatar } from "@/components/Avatar";
import { MODE_GLYPHS } from "@/components/ModeMark";
import { StatusDot } from "@/components/StatusDot";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import { UnreadDot } from "@/components/UnreadDot";
import { formatReceived } from "@/lib/format";
import type { AssistantLink } from "./links";
import type { ModesByThread } from "./threadGlyph";
import { useLinkPreview, type PersonProfile, type ThreadPreview } from "./useLinkPreview";
import { usePreviewOpen } from "./usePreviewOpen";

import type { ThreadModes } from "@/features/modes/membership";

export function LinkHoverCard({
  link,
  title,
  modes,
  children,
}: {
  link: AssistantLink;
  /** The subject or name the chip already shows. */
  title: string;
  /** Where each email lives, already looked up for the reply or rows. */
  modes: ModesByThread;
  /** The link itself: one element that takes a ref and props. */
  children: ReactNode;
}) {
  const { open, dismiss, linkProps, cardProps } = usePreviewOpen();
  const { thread, person } = useLinkPreview(link, open);
  const cardId = useId();
  // The ref's type is the anchor's, though it holds the link; only `contains` is used.
  const anchor = useRef<HTMLDivElement>(null);
  return (
    // Popover, not a hover library: when it opens is decided by usePreviewOpen.
    // The link is the anchor, not a trigger, so Radix adds no click handling.
    <Popover open={open} modal={false}>
      <PopoverAnchor
        asChild
        ref={anchor}
        // The card adds to what the link says (sender, date, snippet), so while
        // it is open it describes the link, as a tooltip does. Hidden from
        // assistive tech instead, that detail would exist only for sighted users.
        aria-describedby={open ? cardId : undefined}
        {...linkProps}
      >
        {children}
      </PopoverAnchor>
      <PopoverContent
        side="left"
        align="start"
        sideOffset={8}
        collisionPadding={8}
        className="w-80 p-3"
        data-testid="link-hover-card"
        id={cardId}
        // Not a dialog: the card has no controls and repeats the link's target,
        // so it is a description. Radix Popover wraps its content in a focus
        // scope that makes it focusable (tabindex -1, so a click on its padding
        // would trap Tab); unsetting tabIndex here keeps focus on the link.
        role="tooltip"
        tabIndex={undefined}
        onOpenAutoFocus={(event) => event.preventDefault()}
        onCloseAutoFocus={(event) => event.preventDefault()}
        // Focus landing on the link that anchors the card is not "outside".
        onInteractOutside={(event) => {
          if (event.target instanceof Node && anchor.current?.contains(event.target)) {
            event.preventDefault();
            return;
          }
          dismiss();
        }}
        {...cardProps}
      >
        {link.kind === "thread" ? (
          <EmailCard title={title} preview={thread} held={modes?.get(link.thread_id)} />
        ) : (
          <PersonCard name={title} page={person} />
        )}
      </PopoverContent>
    </Popover>
  );
}

function EmailCard({
  title,
  preview,
  held,
}: {
  title: string;
  preview: ThreadPreview | null;
  held: ThreadModes | undefined;
}) {
  const sender = preview
    ? preview.sender_name
      ? `${preview.sender_name} <${preview.sender_email}>`
      : preview.sender_email
    : null;
  const count = preview?.message_count ?? 0;
  const modes = held?.modes.length
    ? held.modes.map((entry) => ({ id: entry.mode, name: entry.name }))
    : held?.in_inbox
      ? [{ id: "inbox" as const, name: "Inbox" }]
      : [];
  const unread = preview?.unread_count ?? 0;
  const hasMarks = modes.length > 0 || preview?.has_attachments || unread > 0;
  return (
    <div className="flex flex-col gap-1.5">
      <p className="type-body line-clamp-2 font-semibold text-text-1">
        {preview?.subject ?? title}
      </p>
      {preview ? (
        <>
          <p className="type-meta truncate text-text-1">{sender}</p>
          <p className="type-meta text-text-2">
            {formatReceived(new Date(preview.latest_at * 1000))}
            {` · ${count} ${count === 1 ? "message" : "messages"}`}
          </p>
          {preview.snippet ? (
            <p className="type-body line-clamp-3 text-text-2">{preview.snippet}</p>
          ) : null}
        </>
      ) : null}
      {hasMarks ? (
        <p className="type-meta flex flex-wrap items-center gap-x-3 gap-y-1 pt-0.5 text-text-2">
          {modes.map((entry) => {
            const Glyph = MODE_GLYPHS[entry.id];
            return (
              <span key={entry.id} className="inline-flex items-center gap-1">
                <Glyph aria-hidden className="size-3.5" strokeWidth={1.75} />
                {entry.name}
              </span>
            );
          })}
          {preview?.has_attachments ? (
            <span className="inline-flex items-center gap-1">
              <Paperclip aria-hidden className="size-3.5" strokeWidth={1.75} />
              Attachment
            </span>
          ) : null}
          {unread > 0 ? (
            <span className="inline-flex items-center gap-1.5">
              <UnreadDot aria-hidden />
              {unread} unread
            </span>
          ) : null}
        </p>
      ) : null}
    </div>
  );
}

function PersonCard({ name, page }: { name: string; page: PersonProfile | null }) {
  const shown = page?.row.title ?? name;
  return (
    <div className="flex items-start gap-3">
      <Avatar name={shown} size="md" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <p className="type-heading truncate text-text-1">{shown}</p>
        {page ? (
          <>
            {page.header_line ? <p className="type-meta text-text-2">{page.header_line}</p> : null}
            {page.relationship_line ? (
              <p className="type-meta text-text-2">{page.relationship_line}</p>
            ) : null}
            {page.row.your_turn ? (
              <p className="type-meta inline-flex items-center gap-1.5 pt-0.5 text-text-1">
                <StatusDot aria-hidden className="size-1.5 bg-primary" />
                Your turn
              </p>
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  );
}
