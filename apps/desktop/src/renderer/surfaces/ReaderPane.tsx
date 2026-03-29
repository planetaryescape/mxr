import { Archive, ChevronDown, ChevronRight, Paperclip, X } from "lucide-react";
import { useMemo, useState } from "react";
import type { AttachmentMeta, MailboxRow, ReaderMode, ThreadBody, ThreadResponse } from "../../shared/types";
import { cn } from "../lib/cn";
import { SkeletonReaderBody, SkeletonReaderHeader } from "../lib/skeleton";
import { formatBytes, renderReaderBody, renderReaderParagraphs } from "./formatters";

export function ReaderPane(props: {
  className?: string;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
}) {
  if (!props.thread) {
    return (
      <section className={cn("min-h-0 flex-1 flex-col bg-panel", props.className)}>
        <EmptyReaderState />
      </section>
    );
  }

  return (
    <section className={cn("min-h-0 flex-1 flex-col bg-panel", props.className)}>
      {/* Thread header */}
      <ThreadHeader
        thread={props.thread}
        readerMode={props.readerMode}
        setReaderMode={props.setReaderMode}
        onArchive={props.onArchive}
        onClose={props.onCloseReader}
      />

      {/* Conversation body */}
      <div className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto scroll-smooth">
        <ConversationView
          thread={props.thread}
          readerMode={props.readerMode}
          signatureExpanded={props.signatureExpanded}
        />
      </div>
    </section>
  );
}

function ThreadHeader(props: {
  thread: ThreadResponse;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  onArchive: () => void;
  onClose: () => void;
}) {
  const messageCount = props.thread.messages.length;
  const lastMessage = props.thread.messages[messageCount - 1];

  return (
    <div className="border-b border-outline px-4 py-3">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-[length:var(--text-lg)] font-semibold leading-tight text-foreground">
            {props.thread.thread.subject}
          </h2>
          <p className="mt-1 text-[length:var(--text-xs)] text-foreground-subtle">
            {messageCount} {messageCount === 1 ? "message" : "messages"}
            {lastMessage ? ` · ${lastMessage.date_label}` : ""}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {/* Reader mode tabs */}
          <div className="mr-2 flex items-center gap-0.5">
            {(["auto", "reader", "html", "raw"] as ReaderMode[]).map((mode) => (
              <button
                key={mode}
                className={cn(
                  "px-2 py-1 text-[length:var(--text-xs)] uppercase transition-colors",
                  props.readerMode === mode
                    ? "bg-accent/12 text-accent"
                    : "text-foreground-subtle hover:text-foreground-muted",
                )}
                style={{ borderRadius: "var(--radius-sm)" }}
                onClick={() => props.setReaderMode(mode)}
              >
                {mode}
              </button>
            ))}
          </div>
          <button
            className="flex size-7 items-center justify-center text-foreground-subtle transition-colors hover:text-foreground"
            onClick={props.onArchive}
            title="Archive"
          >
            <Archive className="size-3.5" />
          </button>
          <button
            className="flex size-7 items-center justify-center text-foreground-subtle transition-colors hover:text-foreground"
            onClick={props.onClose}
            title="Close"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}

function ConversationView(props: {
  thread: ThreadResponse;
  readerMode: ReaderMode;
  signatureExpanded: boolean;
}) {
  const { messages, bodies } = props.thread;

  // Build a map from message_id to body
  const bodyMap = useMemo(() => {
    const map = new Map<string, ThreadBody>();
    for (const body of bodies) {
      map.set(body.message_id, body);
    }
    return map;
  }, [bodies]);

  // For multi-message threads, collapse all but the last
  const lastIndex = messages.length - 1;

  if (messages.length === 0) {
    return (
      <div className="px-4 py-6">
        <SkeletonReaderBody />
      </div>
    );
  }

  return (
    <div className="divide-y divide-outline">
      {messages.map((msg, index) => (
        <MessageCard
          key={msg.id}
          message={msg}
          body={bodyMap.get(msg.id) ?? null}
          readerMode={props.readerMode}
          signatureExpanded={props.signatureExpanded}
          defaultExpanded={index === lastIndex}
          isOnly={messages.length === 1}
        />
      ))}
    </div>
  );
}

function MessageCard(props: {
  message: MailboxRow;
  body: ThreadBody | null;
  readerMode: ReaderMode;
  signatureExpanded: boolean;
  defaultExpanded: boolean;
  isOnly: boolean;
}) {
  const [expanded, setExpanded] = useState(props.defaultExpanded);
  const initials = getInitials(props.message.sender);
  const avatarColor = getAvatarColor(props.message.sender);

  return (
    <article className="bg-panel">
      {/* Message header -- always visible, clickable to toggle */}
      <button
        className={cn(
          "flex w-full items-center gap-3 px-4 py-3 text-left transition-colors",
          !props.isOnly && "hover:bg-panel-elevated/40",
        )}
        onClick={() => !props.isOnly && setExpanded(!expanded)}
      >
        {/* Avatar */}
        <div
          className="flex size-8 shrink-0 items-center justify-center text-[length:var(--text-xs)] font-semibold text-white"
          style={{ backgroundColor: avatarColor, borderRadius: "var(--radius-md)" }}
        >
          {initials}
        </div>

        {/* Sender info */}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[length:var(--text-sm)] font-medium text-foreground">
              {props.message.sender}
            </span>
            {props.message.sender_detail ? (
              <span className="truncate text-[length:var(--text-xs)] text-foreground-subtle">
                {props.message.sender_detail}
              </span>
            ) : null}
          </div>
          {!expanded ? (
            <p className="mt-0.5 truncate text-[length:var(--text-xs)] text-foreground-subtle">
              {props.message.snippet}
            </p>
          ) : null}
        </div>

        {/* Date + expand indicator */}
        <div className="flex shrink-0 items-center gap-2">
          <span className="font-mono text-[length:var(--text-xs)] tabular-nums text-foreground-subtle">
            {props.message.date_label}
          </span>
          {!props.isOnly ? (
            expanded ? (
              <ChevronDown className="size-3.5 text-foreground-subtle" />
            ) : (
              <ChevronRight className="size-3.5 text-foreground-subtle" />
            )
          ) : null}
        </div>
      </button>

      {/* Message body -- shown when expanded */}
      {expanded ? (
        <div className="px-4 pb-4 pl-[3.75rem]">
          <MessageBody
            body={props.body}
            readerMode={props.readerMode}
            signatureExpanded={props.signatureExpanded}
            snippet={props.message.snippet}
          />
          {/* Inline attachments */}
          {props.body && props.body.attachments.length > 0 ? (
            <AttachmentList attachments={props.body.attachments} />
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

function MessageBody(props: {
  body: ThreadBody | null;
  readerMode: ReaderMode;
  signatureExpanded: boolean;
  snippet: string;
}) {
  const [remoteContentAllowed, setRemoteContentAllowed] = useState(false);

  if (!props.body) {
    return (
      <p className="text-[length:var(--text-base)] leading-relaxed text-foreground-subtle italic">
        No body available
      </p>
    );
  }

  const htmlBody = props.body.text_html ?? null;
  const plainBody = props.body.text_plain ?? null;
  const rawBody = props.body.raw_source ?? htmlBody ?? plainBody ?? null;

  // HTML mode
  if (props.readerMode === "html" && htmlBody) {
    const sanitizedHtml = remoteContentAllowed ? htmlBody : stripRemoteContent(htmlBody);
    return (
      <div>
        {!remoteContentAllowed ? (
          <button
            className="mb-2 border border-outline bg-canvas-elevated px-2.5 py-1 text-[length:var(--text-xs)] text-foreground-muted transition-colors hover:text-foreground"
            style={{ borderRadius: "var(--radius-sm)" }}
            onClick={() => setRemoteContentAllowed(true)}
          >
            Load remote content
          </button>
        ) : null}

        <iframe
          className="w-full border border-outline bg-white"
          style={{
            minHeight: "24rem",
            height: "calc(100vh - 16rem)",
            borderRadius: "var(--radius-sm)",
          }}
          srcDoc={sanitizedHtml}
          title="HTML message"
          sandbox="allow-same-origin"
        />
      </div>
    );
  }

  // Raw mode
  if (props.readerMode === "raw") {
    return (
      <pre className="max-w-[48rem] whitespace-pre-wrap text-[length:var(--text-sm)] leading-relaxed text-foreground-muted">
        {rawBody ?? "No raw body available"}
      </pre>
    );
  }

  // Auto / Reader mode -- render plain text with styling
  const text = plainBody ?? props.snippet ?? "No readable body";
  const processedBody = renderReaderBody(text, props.signatureExpanded);
  const paragraphs = renderReaderParagraphs(processedBody);

  return (
    <div className="max-w-[48rem] space-y-3">
      {paragraphs.map((paragraph, index) => {
        // Check if this is a quoted block (lines starting with >)
        const isQuoted = paragraph.trimStart().startsWith(">");
        if (isQuoted) {
          return (
            <blockquote
              key={`${index}-${paragraph.slice(0, 20)}`}
              className="border-l-2 border-foreground-subtle/25 pl-3 text-[length:var(--text-base)] leading-relaxed text-foreground-subtle"
            >
              {paragraph.replace(/^>\s?/gm, "")}
            </blockquote>
          );
        }

        return (
          <p
            key={`${index}-${paragraph.slice(0, 20)}`}
            className="whitespace-pre-wrap text-pretty text-[length:var(--text-base)] leading-relaxed text-foreground-muted"
          >
            {paragraph}
          </p>
        );
      })}
    </div>
  );
}

function AttachmentList(props: { attachments: AttachmentMeta[] }) {
  return (
    <div className="mt-3 flex flex-wrap gap-2">
      {props.attachments.map((att) => (
        <div
          key={att.id}
          className="flex items-center gap-2 border border-outline bg-panel-muted px-2.5 py-1.5"
          style={{ borderRadius: "var(--radius-sm)" }}
        >
          <Paperclip className="size-3 shrink-0 text-foreground-subtle" />
          <span className="text-[length:var(--text-xs)] text-foreground-muted">{att.filename}</span>
          <span className="text-[length:var(--text-2xs)] text-foreground-subtle">
            {formatBytes(att.size_bytes)}
          </span>
        </div>
      ))}
    </div>
  );
}

function EmptyReaderState() {
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center bg-panel-muted px-4 py-5">
      <div className="max-w-md text-center">
        <h2 className="text-[length:var(--text-base)] font-medium text-foreground-muted">
          Select a message to read
        </h2>
        <p className="mt-1.5 text-[length:var(--text-xs)] text-foreground-subtle">
          Use j/k to navigate, Enter to open
        </p>
      </div>
    </div>
  );
}

// --- Utilities ---

function getInitials(sender: string): string {
  const name = sender.replace(/<.*>/, "").trim();
  const parts = name.split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

const AVATAR_COLORS = [
  "#4f6d7a", "#5b8a72", "#7a6b5d", "#6b5b7a", "#5b6b7a",
  "#7a5b5b", "#5b7a6b", "#6b7a5b", "#7a6b7a", "#5b7a7a",
];

function getAvatarColor(sender: string): string {
  let hash = 0;
  for (let i = 0; i < sender.length; i++) {
    hash = ((hash << 5) - hash + sender.charCodeAt(i)) | 0;
  }
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length];
}

function stripRemoteContent(html: string): string {
  return html
    .replace(/<img\s[^>]*src=["']https?:\/\/[^"']*["'][^>]*\/?>/gi, "<!-- remote image blocked -->")
    .replace(/url\(["']?https?:\/\/[^)"']*["']?\)/gi, "url()")
    .replace(/<link\s[^>]*href=["']https?:\/\/[^"']*["'][^>]*\/?>/gi, "<!-- remote stylesheet blocked -->");
}
