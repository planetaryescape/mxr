import { ChevronDown, MoreHorizontal, Reply, ReplyAll, Forward, Star } from "lucide-react";
import { forwardRef, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { performMailAction } from "@/features/mail-actions/mailMutations";
import type { AddressView, MessageBodyView, MessageRowView } from "@/features/mailbox/types";
import { formatLongDate, initials, parseAddress, plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs, type ReaderView } from "@/state/uiPrefsStore";

import { AttachmentActions } from "./AttachmentActions";
import { splitHtmlQuote } from "./htmlQuote";
import { useInlineImages } from "./inlineImages";
import { InviteCard } from "./InviteCard";
import { MessageBody } from "./MessageBody";
import { MessageText } from "./MessageText";
import { PrivacyLine } from "./privacy/PrivacyLine";
import { When } from "@/components/When";

export interface MessageCardProps {
  message: MessageRowView;
  body?: MessageBodyView;
  expanded: boolean;
  focused: boolean;
  view: ReaderView;
  showQuotes: boolean;
  showSignature: boolean;
  remoteAllowedForThread: boolean;
  /** The ask's verified quote, when this message makes the ask. */
  askQuote?: string;
  onToggle: () => void;
  onAllowRemote: () => void;
  /** Switch the reader to the formatted view (to show loaded images). */
  onShowFormatted: () => void;
  onShowHeaders: () => void;
}

/**
 * One message in a conversation. Collapsed, it is a single line (sender,
 * snippet, date); expanded, it shows the full header, the invite card if
 * any, the body in the chosen view, and attachments.
 */
export const MessageCard = forwardRef<HTMLElement, MessageCardProps>(function MessageCard(
  {
    message,
    body,
    expanded,
    focused,
    view,
    showQuotes,
    showSignature,
    remoteAllowedForThread,
    askQuote,
    onToggle,
    onAllowRemote,
    onShowFormatted,
    onShowHeaders,
  },
  ref,
) {
  const sender = parseAddress(message.sender_detail ?? message.sender);
  const name = message.sender || sender.name || sender.email || "Unknown sender";
  const email = message.sender_detail ?? sender.email ?? "";

  if (!expanded) {
    return (
      <section
        ref={ref}
        data-testid="thread-message"
        data-message-id={message.id}
        data-focused={focused || undefined}
        data-collapsed="true"
        className={cn("group relative border-b border-border/70", focused && "bg-accent/60")}
      >
        {focused ? (
          <span aria-hidden className="absolute inset-y-0 left-0 w-[3px] bg-primary" />
        ) : null}
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={false}
          className="grid w-full grid-cols-[32px_minmax(0,auto)_minmax(0,1fr)_auto] items-center gap-3 px-5 py-2.5 text-left hover:bg-accent/40"
        >
          <Avatar name={name} small />
          <span
            className={cn(
              "truncate text-[13.5px]",
              message.unread ? "font-semibold" : "font-medium",
            )}
          >
            {name}
          </span>
          <span className="truncate text-[13px] text-muted-foreground">{message.snippet}</span>
          <time
            dateTime={message.date}
            title={message.date_full}
            className="font-mono text-2xs text-muted-foreground tabular-nums"
          >
            <When value={message.date} />
          </time>
        </button>
      </section>
    );
  }

  const attachments = (body?.attachments ?? []).filter((attachment) => !attachment.content_id);
  const calendar = body?.metadata?.calendar;

  return (
    <section
      ref={ref}
      data-testid="thread-message"
      data-message-id={message.id}
      data-focused={focused || undefined}
      className={cn("relative border-b border-border/70", focused && "bg-accent/20")}
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-0 left-0 w-[3px] bg-primary" />
      ) : null}
      <header className="flex items-start gap-3 px-5 pb-2 pt-4">
        <Avatar name={name} />
        <div className="min-w-0 flex-1">
          <button type="button" onClick={onToggle} aria-expanded className="block w-full text-left">
            <span className="flex flex-wrap items-baseline gap-x-2">
              <span className={cn("text-[14px]", message.unread ? "font-semibold" : "font-medium")}>
                {name}
              </span>
              {email && email !== name ? (
                <span className="truncate font-mono text-2xs text-muted-foreground">{email}</span>
              ) : null}
            </span>
          </button>
          <Recipients to={message.to} cc={message.cc} bcc={message.bcc} />
        </div>
        <span className="flex shrink-0 items-center gap-1">
          <time
            dateTime={message.date}
            title={formatLongDate(message.date)}
            className="mr-1 whitespace-nowrap font-mono text-2xs text-muted-foreground tabular-nums"
          >
            <When value={message.date} />
          </time>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={message.starred ? "Unstar message" : "Star message"}
            onClick={() =>
              void performMailAction(message.starred ? "unstar" : "star", [message.id])
            }
          >
            <Star
              className={cn(
                "size-3.5",
                message.starred ? "fill-star text-star" : "text-muted-foreground",
              )}
            />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Reply to this message"
            onClick={() =>
              useComposeUi.getState().openCompose(replyIntent(message.id, "single"), "inline")
            }
          >
            <Reply className="size-3.5 text-muted-foreground" />
          </Button>
          <MessageMenu message={message} onShowHeaders={onShowHeaders} />
        </span>
      </header>

      <div className="px-5 pb-5 pl-[64px]">
        {calendar ? <InviteCard messageId={message.id} metadata={calendar} /> : null}
        <MessageContent
          message={message}
          body={body}
          view={view}
          showQuotes={showQuotes}
          showSignature={showSignature}
          remoteAllowed={remoteAllowedForThread}
          askQuote={askQuote}
          onAllowRemote={onAllowRemote}
          onShowFormatted={onShowFormatted}
          senderEmail={sender.email}
        />
        {attachments.length > 0 ? (
          <div className="mt-4">
            <div className="mb-2 font-mono text-2xs uppercase tracking-wider text-muted-foreground">
              {plural(attachments.length, "attachment")}
            </div>
            <div className="grid gap-2 @2xl:grid-cols-2">
              {attachments.map((attachment) => (
                <AttachmentActions
                  key={attachment.id ?? attachment.filename}
                  attachment={attachment}
                  messageId={body?.message_id}
                />
              ))}
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
});

/**
 * A message's body in the chosen view, with its privacy line. Shared by the
 * reader and Reading's feed, where each issue is already open.
 */
export function MessageContent({
  message,
  body,
  view,
  showQuotes,
  showSignature,
  remoteAllowed,
  askQuote,
  onAllowRemote,
  onShowFormatted,
  senderEmail,
}: {
  message: MessageRowView;
  body?: MessageBodyView;
  view: ReaderView;
  showQuotes: boolean;
  showSignature: boolean;
  remoteAllowed: boolean;
  askQuote?: string;
  onAllowRemote: () => void;
  onShowFormatted: () => void;
  senderEmail: string | null;
}) {
  const emailHtmlTheme = useUiPrefs((s) => s.emailHtmlTheme);
  const trustedSenders = useUiPrefs((s) => s.remoteImageSenders);
  const allowSender = useUiPrefs((s) => s.allowRemoteImagesFrom);
  const [showHtmlQuote, setShowHtmlQuote] = useState(false);
  const html = useInlineImages(body?.message_id, body?.text_html ?? null);
  const parts = useMemo(
    () => (html ? splitHtmlQuote(html, { keepSignature: showSignature }) : null),
    [html, showSignature],
  );
  const senderTrusted = senderEmail ? trustedSenders.includes(senderEmail.toLowerCase()) : false;
  const allowRemote = remoteAllowed || senderTrusted;

  if (!body) {
    return (
      <p className="text-[13px] italic text-muted-foreground">
        {message.snippet || "Loading message…"}
      </p>
    );
  }

  const privacy = html ? (
    <PrivacyLine
      html={html}
      imagesAllowed={allowRemote}
      onShowImages={() => {
        onAllowRemote();
        onShowFormatted();
      }}
      senderEmail={senderEmail}
      onAlwaysAllowSender={allowSender}
    />
  ) : null;

  if (view === "formatted" && html && parts) {
    const expandedQuote = showQuotes || showHtmlQuote;
    const rendered = expandedQuote ? html : parts.main;
    return (
      <div>
        {privacy}
        <MessageBody
          html={rendered}
          allowRemoteImages={allowRemote}
          theme={emailHtmlTheme}
          highlight={askQuote}
          onInteract={() => useMailboxPane.getState().setActivePane("reader")}
        />
        {parts.hasQuote && !showQuotes ? (
          <button
            type="button"
            onClick={() => setShowHtmlQuote((value) => !value)}
            className="mt-2 inline-flex items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2 py-0.5 font-mono text-2xs text-muted-foreground hover:border-border-strong hover:text-foreground"
          >
            <MoreHorizontal className="size-3.5" />
            {showHtmlQuote ? "Hide quoted text" : "Show quoted text"}
          </button>
        ) : null}
      </div>
    );
  }

  const text =
    view === "plain"
      ? (body.text_plain ?? body.reader_text ?? "")
      : (body.reader_text ?? body.text_plain ?? "");
  if (!text.trim()) {
    return html ? (
      <p className="text-[13px] text-muted-foreground">
        This message has no text version. Press <span className="font-mono">H</span> for the
        formatted view.
      </p>
    ) : (
      <p className="text-[13px] italic text-muted-foreground">No readable body.</p>
    );
  }
  return (
    <>
      {privacy}
      <MessageText
        text={text}
        showQuotes={showQuotes}
        showSignature={showSignature}
        plain={view === "plain"}
        highlight={askQuote}
      />
    </>
  );
}

function Recipients({
  to,
  cc,
  bcc,
}: {
  to?: AddressView[];
  cc?: AddressView[];
  bcc?: AddressView[];
}) {
  const [open, setOpen] = useState(false);
  const all = [...(to ?? []), ...(cc ?? [])];
  const summary =
    all.length === 0 ? "undisclosed recipients" : all.slice(0, 3).map(shortName).join(", ");
  const more = all.length > 3 ? ` and ${all.length - 3} more` : "";
  return (
    <span className="mt-0.5 block text-[12.5px] text-muted-foreground">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="inline-flex items-center gap-0.5 text-left hover:text-foreground"
      >
        to {summary}
        {more}
        <ChevronDown className={cn("size-3 transition-transform", open && "rotate-180")} />
      </button>
      {open ? (
        <span className="mt-1 grid grid-cols-[3rem_1fr] gap-x-2 gap-y-0.5 font-mono text-2xs">
          <AddressRow label="to" list={to} />
          <AddressRow label="cc" list={cc} />
          <AddressRow label="bcc" list={bcc} />
        </span>
      ) : null}
    </span>
  );
}

function AddressRow({ label, list }: { label: string; list?: AddressView[] }) {
  if (!list || list.length === 0) return null;
  return (
    <>
      <span className="text-muted-foreground">{label}</span>
      <span className="break-all">
        {list.map((a) => (a.name ? `${a.name} <${a.email}>` : a.email)).join(", ")}
      </span>
    </>
  );
}

function shortName(address: AddressView): string {
  return address.name?.trim() || address.email;
}

function MessageMenu({
  message,
  onShowHeaders,
}: {
  message: MessageRowView;
  onShowHeaders: () => void;
}) {
  const open = (mode: "single" | "all" | "forward") =>
    useComposeUi.getState().openCompose(replyIntent(message.id, mode), "inline");
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-xs" aria-label="More for this message">
          <MoreHorizontal className="size-3.5 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <DropdownMenuItem onSelect={() => open("single")}>
          <Reply className="size-3.5" /> Reply
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => open("all")}>
          <ReplyAll className="size-3.5" /> Reply all
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => open("forward")}>
          <Forward className="size-3.5" /> Forward
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={() => void performMailAction(message.unread ? "read" : "unread", [message.id])}
        >
          Mark {message.unread ? "read" : "unread"} from here
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onShowHeaders}>
          Raw headers <DropdownMenuShortcut>g h</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const AVATAR_TONES = [
  "bg-chart-1/20 text-chart-1",
  "bg-chart-2/20 text-chart-2",
  "bg-chart-3/20 text-chart-3",
  "bg-chart-4/20 text-chart-4",
  "bg-chart-5/20 text-chart-5",
  "bg-chart-6/20 text-chart-6",
];

function Avatar({ name, small = false }: { name: string; small?: boolean }) {
  const hash = [...name].reduce((acc, char) => (acc * 31 + char.charCodeAt(0)) >>> 0, 7);
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center rounded-full font-mono font-semibold",
        small ? "size-7 text-[11px]" : "size-8 text-[12px]",
        AVATAR_TONES[hash % AVATAR_TONES.length],
      )}
    >
      {initials(name)}
    </span>
  );
}
