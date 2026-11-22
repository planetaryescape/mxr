import { Action, Color, Icon, List } from "@raycast/api";
import { useCachedState, usePromise } from "@raycast/utils";
import type { ReactNode } from "react";
import { useState } from "react";
import { type Thread, getThread } from "../bridge/mail";
import type { AccountTag } from "./accountBadge";
import { bodyMarkdown, inline } from "./body";
import { senderFace } from "./faces";
import { keyFor } from "./keys";
import { message } from "./previewed";
import { fullDate } from "./time";

export function usePane(command: string) {
  const [showing, setShowing] = useCachedState<boolean>(`pane:${command}`, false);
  const [selected, setSelected] = useState<string>();
  return {
    showing,
    selected,
    toggle: () => setShowing((was) => !was),
    select: (id: string | null) => setSelected(id ?? undefined),
  };
}

export type Pane = ReturnType<typeof usePane>;

export function PaneAction({ pane }: { pane: Pane }) {
  return (
    <Action
      title={pane.showing ? "Hide Reading Pane" : "Show Reading Pane"}
      icon={Icon.Sidebar}
      shortcut={keyFor("raycast.reading-pane")}
      onAction={pane.toggle}
    />
  );
}

const SEARCH_ROW = "search-";

export function searchRowId(threadId: string): string {
  return `${SEARCH_ROW}${threadId}`;
}

export function searchRowThread(id: string | undefined): string | undefined {
  return id?.startsWith(SEARCH_ROW) ? id.slice(SEARCH_ROW.length) : undefined;
}

export interface Preview {
  thread?: Thread;
  loading: boolean;
  error?: Error;
}

export function usePreview(threadId: string | undefined): Preview {
  const { data, isLoading, error } = usePromise(
    async (id: string) => ({ id, thread: await getThread(id) }),
    [threadId ?? ""],
    { execute: Boolean(threadId), onError: () => undefined },
  );
  if (!threadId) return { loading: false };
  return {
    thread: data?.id === threadId ? data.thread : undefined,
    loading: isLoading,
    error: isLoading ? undefined : error,
  };
}

interface MailPaneProps {
  subject: string;
  fromName: string;
  fromEmail?: string | null;
  date?: string | null;
  account?: AccountTag;
  unread?: boolean;
  preview?: Preview;
  fallback?: string | null;
  children?: ReactNode;
}

export function MailPane({
  subject,
  fromName,
  fromEmail,
  date,
  account,
  unread,
  preview,
  fallback,
  children,
}: MailPaneProps) {
  const thread = preview?.thread;
  const latest = thread?.messages.at(-1);
  const body = latest ? thread?.bodies.find((entry) => entry.message_id === latest.id) : undefined;
  const text = preview?.error
    ? `**Could not load this conversation.** ${inline(message(preview.error))}`
    : latest
      ? bodyMarkdown(body)
      : fallback
        ? inline(fallback)
        : "";
  const count = thread?.messages.length ?? 0;
  const to = latest?.to?.map((address) => address.name || address.email).join(", ");
  return (
    <List.Item.Detail
      isLoading={preview?.loading}
      markdown={`## ${inline(subject) || "(no subject)"}\n\n${text}`}
      metadata={
        <List.Item.Detail.Metadata>
          <List.Item.Detail.Metadata.Label
            title="From"
            text={fromEmail && fromEmail !== fromName ? `${fromName} <${fromEmail}>` : fromName}
            icon={senderFace(fromName, fromEmail)}
          />
          {to ? <List.Item.Detail.Metadata.Label title="To" text={to} /> : null}
          {date ? <List.Item.Detail.Metadata.Label title="Received" text={fullDate(date)} /> : null}
          {children}
          {account || unread || count > 1 ? <List.Item.Detail.Metadata.Separator /> : null}
          {account ? (
            <List.Item.Detail.Metadata.TagList title="Account">
              <List.Item.Detail.Metadata.TagList.Item text={account.label} color={account.color} />
            </List.Item.Detail.Metadata.TagList>
          ) : null}
          {unread || count > 1 ? (
            <List.Item.Detail.Metadata.TagList title="Conversation">
              {unread ? (
                <List.Item.Detail.Metadata.TagList.Item text="Unread" color={Color.Blue} />
              ) : null}
              {count > 1 ? (
                <List.Item.Detail.Metadata.TagList.Item text={`${count} messages`} />
              ) : null}
            </List.Item.Detail.Metadata.TagList>
          ) : null}
        </List.Item.Detail.Metadata>
      }
    />
  );
}
