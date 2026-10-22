import { Action, ActionPanel, Color, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getMessages, markDone } from "./bridge/modes";
import type { MessagesRow } from "./bridge/types";
import { AccountDropdown } from "./components/AccountDropdown";
import { useLiveRefresh } from "./components/live";
import { SearchSection, useMailSearch } from "./components/MailRow";
import { ReplyForm } from "./components/ReplyForm";
import { ReadAction } from "./components/ThreadDetail";
import { itemErrors, runPreviewed } from "./components/previewed";
import { arrival } from "./components/time";
import { useWebBase, web } from "./components/web";

export default function Messages() {
  const [account, setAccount] = useState<string>();
  const { data, isLoading, revalidate } = useCachedPromise(getMessages, [account], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();
  const search = useMailSearch(account);
  const shown = (rows: MessagesRow[] | undefined) =>
    (rows ?? []).filter((row) =>
      search.keep(
        row.title,
        ...(row.person?.addresses ?? []),
        ...row.topics.map((topic) => topic.subject),
      ),
    );

  const item = (row: MessagesRow, band: string) => {
    const topic = row.topics[0];
    const name = row.person?.name ?? row.title;
    return (
      <List.Item
        key={`${band}-${row.account_id}-${row.id}`}
        icon={
          row.overdue
            ? { source: Icon.Clock, tintColor: Color.Orange }
            : row.kind === "group"
              ? Icon.TwoPeople
              : Icon.Person
        }
        title={row.title}
        subtitle={topic?.subject}
        accessories={[
          ...(row.unread
            ? [{ icon: { source: Icon.Dot, tintColor: Color.Blue }, tooltip: "Unread" }]
            : []),
          ...(row.pace_label ? [{ text: row.pace_label }] : []),
          arrival(row.last_at, row.why),
        ]}
        actions={
          <ActionPanel>
            {topic && (
              <ReadAction
                threadId={topic.thread_id}
                webPath={web.person(row.id, topic.thread_id)}
                replyTo={{ messageId: topic.reply_to_message_id, to: name }}
                onChange={revalidate}
              />
            )}
            {topic && (
              <Action.Push
                title="Reply"
                icon={Icon.Reply}
                shortcut={{ modifiers: ["cmd"], key: "r" }}
                target={
                  <ReplyForm
                    messageId={topic.reply_to_message_id}
                    threadId={topic.thread_id}
                    to={name}
                    onSent={revalidate}
                  />
                }
              />
            )}
            {topic && row.kind !== "person" && (
              <Action.Push
                title="Reply All"
                icon={Icon.Reply}
                shortcut={{ modifiers: ["cmd", "shift"], key: "r" }}
                target={
                  <ReplyForm
                    messageId={topic.reply_to_message_id}
                    threadId={topic.thread_id}
                    kind="reply_all"
                    to={name}
                    onSent={revalidate}
                  />
                }
              />
            )}
            {webBase && (
              <Action.OpenInBrowser
                title="Open in mxr"
                url={`${webBase}${web.person(row.id, topic?.thread_id)}`}
                shortcut={Keyboard.Shortcut.Common.Open}
              />
            )}
            {row.topics.length > 0 && (
              <Action
                title="Done Here"
                icon={Icon.Checkmark}
                shortcut={Keyboard.Shortcut.Common.Edit}
                onAction={() => {
                  const threads = row.topics.map((t) => t.thread_id);
                  return runPreviewed({
                    title: `Done here with ${name}?`,
                    confirm: "Done Here",
                    preview: () => markDone("messages", threads, true),
                    commit: () => markDone("messages", threads, false),
                    describe: (done) => done.items.map((outcome) => outcome.copy).join("\n"),
                    failures: (done) => itemErrors(done.items),
                    undoId: (done) => done.mutation_id,
                    done: "Done here",
                    refresh: revalidate,
                  });
                }}
              />
            )}
          </ActionPanel>
        }
      />
    );
  };

  return (
    <List
      isLoading={isLoading || search.isLoading}
      navigationTitle={data?.header ?? "Messages"}
      searchBarPlaceholder="Filter people and search all mail"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
      {...search.listProps}
    >
      {data && data.row_count === 0 && !search.searching && (
        <List.EmptyView icon={Icon.Message} title={data.empty_state ?? "No conversations"} />
      )}
      <List.Section title="Your Turn">
        {shown(data?.your_turn).map((row) => item(row, "turn"))}
      </List.Section>
      <List.Section title="Pinned">
        {shown(data?.pinned).map((row) => item(row, "pinned"))}
      </List.Section>
      <List.Section title="Recent" subtitle={data ? String(data.recent_total) : undefined}>
        {shown(data?.recent).map((row) => item(row, "recent"))}
      </List.Section>
      <List.Section title="Quiet" subtitle={data ? String(data.quiet_total) : undefined}>
        {shown(data?.quiet).map((row) => item(row, "quiet"))}
      </List.Section>
      <SearchSection search={search} onChange={revalidate} />
    </List>
  );
  useLiveRefresh(revalidate);
}
