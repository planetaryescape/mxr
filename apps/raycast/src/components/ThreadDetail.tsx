import { Action, ActionPanel, Detail, Icon, Toast, showToast } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useEffect, useRef } from "react";
import { type Address, type Thread, getCodes, getThread, setRead } from "../bridge/mail";
import { type AccountTag, useAccountBadge } from "./accountBadge";
import { bodyMarkdown, inline } from "./body";
import { message } from "./previewed";
import { ReplyForm } from "./ReplyForm";
import { fileSize, labelColor, threadAttachments, threadLabels } from "./threadFacts";
import { fullDate } from "./time";
import { useWebBase } from "./web";
import { keyFor } from "./keys";

const READ_AFTER_MS = 600;

interface Props {
  threadId: string;
  webPath: string;
  replyTo?: { messageId: string; to: string };
  onChange?: () => void;
}

function people(addresses: Address[] | undefined): string {
  return (addresses ?? []).map((address) => inline(address.name || address.email)).join(", ");
}

function markdown(data: Thread): string {
  const bodies = new Map(data.bodies.map((body) => [body.message_id, body]));
  const messages = [...data.messages].reverse().map((entry) => {
    const to = people(entry.to);
    const cc = people(entry.cc);
    const attachments = bodies.get(entry.id)?.attachments ?? [];
    return [
      `## ${inline(entry.sender)}`,
      [`${fullDate(entry.date)}`, to && `To ${to}`, cc && `Cc ${cc}`].filter(Boolean).join(" · "),
      bodyMarkdown(bodies.get(entry.id)),
      attachments.length > 0 &&
        `**Attachments:** ${attachments.map((attachment) => inline(attachment.filename)).join(", ")}`,
    ]
      .filter(Boolean)
      .join("\n\n");
  });
  return [`# ${inline(data.thread.subject) || "(no subject)"}`, ...messages].join("\n\n---\n\n");
}

function Panel({ data, account }: { data: Thread; account?: AccountTag }) {
  const labels = threadLabels(data);
  const attachments = threadAttachments(data);
  return (
    <Detail.Metadata>
      {account ? (
        <Detail.Metadata.TagList title="Account">
          <Detail.Metadata.TagList.Item text={account.label} color={account.color} />
        </Detail.Metadata.TagList>
      ) : null}
      {labels.length > 0 ? (
        <Detail.Metadata.TagList title="Labels">
          {labels.map((label) => (
            <Detail.Metadata.TagList.Item
              key={label.id}
              text={label.name}
              color={labelColor(label)}
            />
          ))}
        </Detail.Metadata.TagList>
      ) : null}
      {attachments.map((attachment, index) => (
        <Detail.Metadata.Label
          key={attachment.id}
          title={index === 0 ? "Attachments" : ""}
          text={`${attachment.filename} · ${fileSize(attachment.size_bytes)}`}
          icon={Icon.Paperclip}
        />
      ))}
    </Detail.Metadata>
  );
}

export function ThreadDetail({ threadId, webPath, replyTo, onChange }: Props) {
  const webBase = useWebBase();
  const accounts = useAccountBadge(undefined);
  const { data, isLoading, error } = useCachedPromise(getThread, [threadId]);
  const { data: codes } = useCachedPromise(() => getCodes(undefined, 10), [], {
    onError: () => undefined,
  });
  const code = codes?.find((entry) => entry.thread_id === threadId && entry.current);
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
  }, [data?.thread.id]);

  const account = accounts.tagOf(data?.thread.account_id);
  const hasPanel =
    data && (account || threadLabels(data).length > 0 || threadAttachments(data).length > 0);
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
      metadata={hasPanel ? <Panel data={data} account={account} /> : undefined}
      actions={
        <ActionPanel>
          <ActionPanel.Section>
            {code && (
              <Action.CopyToClipboard
                title={`Copy Code ${code.code}`}
                icon={Icon.Key}
                content={code.code}
                concealed
              />
            )}
            {answer && (
              <Action.Push
                title="Reply"
                icon={Icon.Reply}
                shortcut={keyFor("mail.reply")}
                target={<ReplyForm messageId={answer.messageId} to={answer.to} onSent={onChange} />}
              />
            )}
            {answer && (
              <Action.Push
                title="Reply All"
                icon={Icon.Reply}
                shortcut={keyFor("mail.reply-all")}
                target={
                  <ReplyForm
                    messageId={answer.messageId}
                    kind="reply_all"
                    to={answer.to}
                    onSent={onChange}
                  />
                }
              />
            )}
          </ActionPanel.Section>
          <ActionPanel.Section>
            {webBase && (
              <Action.OpenInBrowser
                title="Open in mxr"
                url={`${webBase}${webPath}`}
                shortcut={keyFor("list.open")}
              />
            )}
            {latest && (
              <Action
                title="Mark as Unread"
                icon={Icon.Dot}
                shortcut={keyFor("mail.mark-unread")}
                onAction={markUnread}
              />
            )}
          </ActionPanel.Section>
        </ActionPanel>
      }
    />
  );
}

export function ReadAction({ title = "Read", ...props }: Props & { title?: string }) {
  return <Action.Push title={title} icon={Icon.Eye} target={<ThreadDetail {...props} />} />;
}
