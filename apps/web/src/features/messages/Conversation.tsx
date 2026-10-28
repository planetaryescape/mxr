import { useQuery } from "@tanstack/react-query";
import { Paperclip } from "lucide-react";
import { useId, useState } from "react";

import { Received } from "@/components/When";
import { KeyChip } from "@/components/KeyChip";
import { fetchThread } from "@/features/mailbox/api";
import { MessageBody } from "@/features/thread/MessageBody";
import { emailHtmlThemeForSender, useUiPrefs } from "@/state/uiPrefsStore";

import { AddressDisclosure } from "./AddressDisclosure";
import { MessageInfoDialog } from "./MessageInfoDialog";
import type { Conversation as ConversationData, ConversationMessage } from "./api";
import { letterLead, paragraphBlocks, splitAsk } from "./messagesView";

/**
 * One topic as a conversation of new texts: each message a ruled section in
 * the reading voice, the sender and time in meta above it (docs/blueprint/28
 * "Messages"). A long message opens on its first paragraph, or the one holding
 * the ask; the quoted history is folded to one line, and the message as sent
 * is one key away. `data-layout` still carries the daemon's length call.
 */
export function ConversationView({
  conversation,
  asSent,
  onToggleAsSent,
  disclosuresEnabled = true,
}: {
  conversation: ConversationData;
  /** The message shown as sent, if any. */
  asSent: string | null;
  onToggleAsSent: (messageId: string) => void;
  /** False while the selected page is placeholder data for another topic/account. */
  disclosuresEnabled?: boolean;
}) {
  return (
    <ol data-testid="conversation" className="flex flex-col px-5 py-2">
      {conversation.earlier_count > 0 ? (
        <li className="type-meta py-2 text-text-3">
          {conversation.earlier_count} earlier messages
        </li>
      ) : null}
      {conversation.messages.map((message, index) => (
        <MessageItem
          key={`${conversation.thread_id}:${message.message_id}`}
          message={message}
          latest={index === conversation.messages.length - 1}
          threadId={conversation.thread_id}
          asSent={asSent === message.message_id}
          disclosuresEnabled={disclosuresEnabled}
          onToggleAsSent={() => onToggleAsSent(message.message_id)}
        />
      ))}
    </ol>
  );
}

function who(message: ConversationMessage): string {
  if (message.from_me) return "You";
  return message.from.name?.split(/\s+/)[0] ?? message.from.email;
}

function MessageItem({
  message,
  latest,
  threadId,
  asSent,
  disclosuresEnabled,
  onToggleAsSent,
}: {
  message: ConversationMessage;
  latest: boolean;
  threadId: string;
  asSent: boolean;
  disclosuresEnabled: boolean;
  onToggleAsSent: () => void;
}) {
  const [expandedOverride, setExpandedOverride] = useState<boolean | null>(null);
  const bodyId = useId();
  const { lead, hidden } = letterLead(message);
  const expanded = expandedOverride ?? latest;
  const shown = expanded || hidden === 0 ? message.text : lead;
  const mine = message.from_me;
  return (
    <li
      data-testid="conversation-message"
      data-latest={latest ? "true" : undefined}
      data-layout={message.layout}
      data-from-me={mine ? "true" : "false"}
      className="flex flex-col items-start border-t border-rule-soft py-4 first:border-t-0"
    >
      <div
        data-testid="message-meta"
        className="type-meta mb-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-text-3"
      >
        <span className="text-text-2">
          {disclosuresEnabled ? (
            <AddressDisclosure
              trigger={who(message)}
              label={`Show original sender address for ${message.from.name ?? who(message)} at ${message.date}`}
              heading="Original sender"
              name={message.from.name}
              address={message.from.email}
            />
          ) : (
            who(message)
          )}
        </span>
        <span aria-hidden>·</span>
        <Received value={message.date} />
        {disclosuresEnabled ? <MessageInfoDialog threadId={threadId} message={message} /> : null}
      </div>
      {asSent ? (
        <AsSent
          threadId={threadId}
          messageId={message.message_id}
          senderEmail={message.from.email}
        />
      ) : message.only_quoted ? (
        // Nothing new in it: the placeholder says so, and the message as
        // sent (the marker below, or v) is how to read it.
        <p data-testid="only-quoted" className="type-body italic text-text-3">
          {message.text}
        </p>
      ) : (
        <div id={bodyId} className="type-reading w-full max-w-[66ch] text-text-1">
          {paragraphBlocks(shown).map((block) => (
            <p key={block.at} className="whitespace-pre-wrap [&+p]:mt-3">
              {splitAsk(block.text, message.ask_quote).map((part) =>
                part.ask ? (
                  <mark
                    key={part.at}
                    data-testid="ask-quote"
                    className="rounded-sm bg-signal-soft px-0.5 text-text-1"
                  >
                    {part.text}
                  </mark>
                ) : (
                  <span key={part.at}>{part.text}</span>
                ),
              )}
            </p>
          ))}
        </div>
      )}
      {!asSent && message.source === "provider_snippet" ? (
        <p data-testid="provider-preview" className="type-meta mt-1 text-text-3">
          {message.text.trim()
            ? "Provider preview only; the full message is unavailable."
            : "No provider preview is available; the full message is unavailable."}{" "}
          <button
            type="button"
            onClick={onToggleAsSent}
            className="underline underline-offset-2 hover:text-foreground"
          >
            Check the original
          </button>
        </p>
      ) : null}
      <div className="type-meta mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-text-3">
        {!asSent && hidden > 0 ? (
          <button
            type="button"
            aria-controls={bodyId}
            aria-expanded={expanded}
            onClick={() => setExpandedOverride(!expanded)}
            className="hover:text-text-1 hover:underline"
          >
            {expanded ? "Show less" : `Read all ${message.paragraphs} paragraphs`}
          </button>
        ) : null}
        {(message.attachments ?? []).map((attachment) => (
          <span key={attachment.attachment_id} className="inline-flex items-center gap-1">
            <Paperclip className="size-3" aria-hidden /> {attachment.filename} ·{" "}
            {Math.max(1, Math.ceil(attachment.size_bytes / 1024))} KB
          </span>
        ))}
        {message.trimmed_label || message.only_quoted || asSent ? (
          <button
            type="button"
            data-testid="trimmed-marker"
            onClick={onToggleAsSent}
            title="Show the message as sent (o or v)"
            className="inline-flex items-center gap-1 hover:text-text-1"
          >
            {asSent ? "back to what they wrote" : (message.trimmed_label ?? "show it as sent")}
            <KeyChip className="h-4 px-1">v</KeyChip>
          </button>
        ) : null}
      </div>
    </li>
  );
}

/** The message as it arrived: its HTML, else its plain text. */
function AsSent({
  threadId,
  messageId,
  senderEmail,
}: {
  threadId: string;
  messageId: string;
  senderEmail: string;
}) {
  const globalTheme = useUiPrefs((state) => state.emailHtmlTheme);
  const senderOverrides = useUiPrefs((state) => state.emailHtmlSenderOverrides);
  const theme = emailHtmlThemeForSender(senderOverrides, senderEmail) ?? globalTheme;
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    staleTime: 60_000,
  });
  const body = thread.data?.bodies.find((candidate) => candidate.message_id === messageId);
  if (!body) {
    return (
      <p data-testid="as-sent" className="text-sm text-muted-foreground">
        {thread.isError
          ? "Couldn't load the message as sent."
          : thread.isSuccess
            ? "The full message is unavailable."
            : "Loading the message as sent…"}
      </p>
    );
  }
  return (
    <div data-testid="as-sent" className="w-full rounded-md border border-border">
      {body.text_html ? (
        <MessageBody html={body.text_html} theme={theme} />
      ) : (
        <pre className="whitespace-pre-wrap p-3 font-sans text-sm leading-6">
          {body.text_plain ?? ""}
        </pre>
      )}
    </div>
  );
}
