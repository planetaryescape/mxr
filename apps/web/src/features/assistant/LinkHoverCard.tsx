/*
 * The preview an email or person chip (or a reply row's link) shows on
 * hover or keyboard focus, only inside the assistant panel (docs/blueprint/29
 * "The copy"). The trigger stays the plain link, so a click or Enter still
 * navigates; the card has no buttons. It opens at once with what the chip
 * knows (a subject or a name) and fills in as the data arrives, with no
 * spinner. Radix ignores touch, so a tap just follows the link. Everything
 * here is mail or model text and is only ever set as text.
 */

import { Paperclip } from "lucide-react";
import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";

import { Avatar } from "@/components/Avatar";
import { MODE_GLYPHS } from "@/components/ModeMark";
import { StatusDot } from "@/components/StatusDot";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card";
import { UnreadDot } from "@/components/UnreadDot";
import { formatReceived } from "@/lib/format";
import type { AssistantLink } from "./links";
import type { ModesByThread } from "./threadGlyph";
import { useLinkPreview, type PersonProfile, type ThreadPreview } from "./useLinkPreview";

import type { ThreadModes } from "@/features/modes/membership";

const OPEN_DELAY_MS = 400;
const CLOSE_DELAY_MS = 200;

/**
 * The one timer that opens a card. A hover or a keyboard focus starts it
 * (restarting any pending one); it opens only if the link is still hovered or
 * focused and not suppressed. Touch never starts it.
 *
 * Activating the link (pointerdown, Enter, Space, click) clears the timer,
 * closes the card and suppresses opening until a new session: one that starts
 * after the pointer has left AND the link has blurred. "Ended" is derived from
 * the hovered and focused flags, which leave and blur always update, so
 * nothing waits on a pointerup or click that may never arrive (a press that is
 * released outside, or cancelled, still ends when the pointer leaves and the
 * link blurs). The focus a press causes comes after its pointerdown and before
 * any leave, so it never counts as a new session.
 */
function useOpenSession(setOpen: (open: boolean) => void) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hovered = useRef(false);
  const focused = useRef(false);
  const suppressed = useRef(false);
  const ended = useRef(false);
  const stop = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => stop, []);

  const begin = () => {
    if (suppressed.current && ended.current) {
      suppressed.current = false;
      ended.current = false;
    }
    if (suppressed.current) return;
    stop();
    timer.current = setTimeout(() => {
      timer.current = null;
      if (!suppressed.current && (hovered.current || focused.current)) setOpen(true);
    }, OPEN_DELAY_MS);
  };
  const leave = () => {
    if (hovered.current || focused.current) return;
    stop();
    if (suppressed.current) ended.current = true;
  };
  const activate = () => {
    stop();
    setOpen(false);
    suppressed.current = true;
    ended.current = false;
  };
  return {
    onPointerEnter: (event: PointerEvent) => {
      if (event.pointerType === "touch") return;
      hovered.current = true;
      begin();
    },
    onPointerLeave: () => {
      hovered.current = false;
      leave();
    },
    onFocus: () => {
      focused.current = true;
      begin();
    },
    onBlur: () => {
      focused.current = false;
      leave();
    },
    onPointerDown: activate,
    onClick: activate,
    onKeyDown: (event: { key: string }) => {
      if (event.key === "Enter" || event.key === " ") activate();
    },
  };
}

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
  const [open, setOpen] = useState(false);
  const { thread, person } = useLinkPreview(link, open);
  const session = useOpenSession(setOpen);
  return (
    <HoverCard
      open={open}
      // Controlled, and only ever told to close: Radix runs a separate open
      // timer for hover and for focus and can't cancel both, so opening is
      // decided here (useOpenSession). Its close handling (leave grace, Esc,
      // outside press) is kept.
      onOpenChange={(next) => {
        if (!next) setOpen(false);
      }}
      openDelay={OPEN_DELAY_MS}
      closeDelay={CLOSE_DELAY_MS}
    >
      <HoverCardTrigger asChild {...session}>
        {children}
      </HoverCardTrigger>
      <HoverCardContent
        side="left"
        collisionPadding={8}
        data-testid="link-hover-card"
        // Esc closes the card and nothing else: the event is marked handled so
        // the narrow-screen drawer, which also closes on Esc, leaves the
        // assistant open.
        onEscapeKeyDown={(event) => {
          event.preventDefault();
          setOpen(false);
        }}
      >
        {link.kind === "thread" ? (
          <EmailCard title={title} preview={thread} held={modes?.get(link.thread_id)} />
        ) : (
          <PersonCard name={title} page={person} />
        )}
      </HoverCardContent>
    </HoverCard>
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
