import { Action, ActionPanel, Detail, Icon, Keyboard, Toast, showToast } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useEffect, useRef } from "react";
import { type Address, type Thread, getThread, setRead } from "../bridge/mail";
import { bodyMarkdown } from "./body";
import { message } from "./previewed";
import { ReplyForm } from "./ReplyForm";
import { fullDate } from "./time";
import { useWebBase } from "./web";

/** As the web reader does when it has focus: read once it has been open this long. */
const READ_AFTER_MS = 600;

interface Props {
  threadId: string;
  /** Path of this conversation in the web app. */
  webPath: string;
  /** The message a reply answers, when the list knows it; else the newest. */
  replyTo?: { messageId: string; to: string };
  /** Reload the list the reader was opened from, once its read state changed. */
  onChange?: () => void;
}

function people(addresses: Address[] | undefined): string {
  return (addresses ?? []).map((address) => address.name || address.email).join(", ");
}

function markdown(data: Thread): string {
  const bodies = new Map(data.bodies.map((body) => [body.message_id, body]));
  // Newest first: the message you opened it for is at the top.
  const messages = [...data.messages].reverse().map((entry) => {
    const to = people(entry.to);
    const cc = people(entry.cc);
    const attachments = bodies.get(entry.id)?.attachments ?? [];
    return [
      `## ${entry.sender}`,
      [`${fullDate(entry.date)}`, to && `To ${to}`, cc && `Cc ${cc}`].filter(Boolean).join(" · "),
      bodyMarkdown(bodies.get(entry.id)),
      attachments.length > 0 &&
        `**Attachments:** ${attachments.map((attachment) => attachment.filename).join(", ")}`,
    ]
      .filter(Boolean)
      .join("\n\n");
  });
  return [`# ${data.thread.subject || "(no subject)"}`, ...messages].join("\n\n---\n\n");
}

/** Read a conversation in Raycast, newest message first. */
export function ThreadDetail({ threadId, webPath, replyTo, onChange }: Props) {
  const webBase = useWebBase();
  const { data, isLoading, error } = useCachedPromise(getThread, [threadId]);
  const markedRead = useRef(false);

  const unread = data?.messages.filter((entry) => entry.unread).map((entry) => entry.id) ?? [];
  useEffect(() => {
    if (markedRead.current || unread.length === 0) return;
    const timer = setTimeout(() => {
      markedRead.current = true;
      setRead(unread, true)
        .then(() => onChange?.())
        .catch((failure) =>
          showToast({
            style: Toast.Style.Failure,
            title: "Not marked read",
            message: message(failure),
          }),
        );
    }, READ_AFTER_MS);
    return () => clearTimeout(timer);
    // Once per opening, when the thread first arrives.
  }, [data?.thread.id]);

  const latest = data?.messages.at(-1);
  const answer = replyTo ?? (latest && { messageId: latest.id, to: latest.sender });
  const markUnread = async () => {
    if (!latest) return;
    try {
      await setRead([latest.id], false);
      markedRead.current = true;
      onChange?.();
      await showToast({ style: Toast.Style.Success, title: "Marked unread" });
    } catch (failure) {
      await showToast({
        style: Toast.Style.Failure,
        title: "Not marked unread",
        message: message(failure),
      });
    }
  };

  return (
    <Detail
      isLoading={isLoading}
      navigationTitle={data?.thread.subject ?? "Conversation"}
      markdown={
        error
          ? `**Could not open this conversation.** ${message(error)}`
          : data
            ? markdown(data)
            : ""
      }
      actions={
        <ActionPanel>
          {answer && (
            <Action.Push
              title="Reply"
              icon={Icon.Reply}
              shortcut={{ modifiers: ["cmd"], key: "r" }}
              target={
                <ReplyForm
                  messageId={answer.messageId}
                  threadId={threadId}
                  to={answer.to}
                  onSent={onChange}
                />
              }
            />
          )}
          {answer && (
            <Action.Push
              title="Reply All"
              icon={Icon.Reply}
              shortcut={{ modifiers: ["cmd", "shift"], key: "r" }}
              target={
                <ReplyForm
                  messageId={answer.messageId}
                  threadId={threadId}
                  kind="reply_all"
                  to={answer.to}
                  onSent={onChange}
                />
              }
            />
          )}
          {webBase && (
            <Action.OpenInBrowser
              title="Open in mxr"
              url={`${webBase}${webPath}`}
              shortcut={Keyboard.Shortcut.Common.Open}
            />
          )}
          {latest && (
            <Action
              title="Mark as Unread"
              icon={Icon.Dot}
              shortcut={{ modifiers: ["cmd", "shift"], key: "u" }}
              onAction={markUnread}
            />
          )}
        </ActionPanel>
      }
    />
  );
}

/** "Read" for a list row: the conversation in Raycast, as the default action. */
export function ReadAction({ title = "Read", ...props }: Props & { title?: string }) {
  return <Action.Push title={title} icon={Icon.Eye} target={<ThreadDetail {...props} />} />;
}
