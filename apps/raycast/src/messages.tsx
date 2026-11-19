import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getMessages, markDone, type MessagesFilter } from "./bridge/modes";
import type { MessagesRow } from "./bridge/types";
import { AccountDropdown } from "./components/AccountDropdown";
import { useAccountBadge } from "./components/accountBadge";
import { useLiveRefresh } from "./components/live";
import { SearchSection, UNREAD, useMailSearch } from "./components/MailRow";
import { senderFace } from "./components/faces";
import { MailPane, PaneAction, searchRowThread, usePane, usePreview } from "./components/pane";
import { ReplyForm } from "./components/ReplyForm";
import { ReadAction } from "./components/ThreadDetail";
import { itemErrors, runPreviewed } from "./components/previewed";
import { arrival } from "./components/time";
import { useWebBase, web } from "./components/web";
import { keyFor } from "./components/keys";

const TURN_CHOICES: [MessagesFilter["turn"], string][] = [
  [undefined, "Needs Attention"],
  ["all", "All Conversations"],
  ["mine", "Your Turn"],
  ["theirs", "Waiting on Them"],
];

/** "Your Turn · Unread": the words for the filters that are on, or nothing. */
function filterLabel(filter: MessagesFilter): string | undefined {
  const parts = [
    TURN_CHOICES.find(([turn]) => turn === filter.turn && turn !== undefined)?.[1] ?? "",
    filter.unread ? "Unread" : "",
    filter.pace ? "Past your pace" : "",
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

function rowId(band: string, row: MessagesRow): string {
  return `${band}-${row.account_id}-${row.id}`;
}

export default function Messages() {
  const [account, setAccount] = useState<string>();
  // Reopening Messages starts with attention, never a remembered history view.
  const [filter, setFilter] = useState<MessagesFilter>({});
  const { data, isLoading, revalidate } = useCachedPromise(getMessages, [account, filter], {
    keepPreviousData: false,
  });
  const label = filterLabel(filter);
  const narrowedEmpty =
    Boolean(label) && data !== undefined && (data.matched ?? data.row_count) === 0;
  const badge = useAccountBadge(account);
  useLiveRefresh(revalidate);
  const pane = usePane("messages");
  // The pane reads each person's most pressing topic.
  const bands: [string, MessagesRow[] | undefined][] = [
    ["turn", data?.your_turn],
    ["pinned", data?.pinned],
    ["recent", data?.recent],
    ["quiet", data?.quiet],
  ];
  const topicThreads = new Map(
    bands.flatMap(([band, rows]) =>
      (rows ?? []).flatMap((row) =>
        row.topics[0] ? [[rowId(band, row), row.topics[0].thread_id] as const] : [],
      ),
    ),
  );
  const selectedThread =
    searchRowThread(pane.selected) ?? (pane.selected ? topicThreads.get(pane.selected) : undefined);
  const preview = usePreview(pane.showing ? selectedThread : undefined);
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

  // ⌘F in every row's panel: the choices, with a check on the ones in force.
  const filterMenu = (
    <ActionPanel.Submenu title="Filter" icon={Icon.Filter} shortcut={keyFor("messages.filter")}>
      {TURN_CHOICES.map(([turn, title]) => (
        <Action
          key={title}
          title={title}
          icon={filter.turn === turn ? Icon.Checkmark : Icon.Circle}
          onAction={() => setFilter({ ...filter, turn })}
        />
      ))}
      <Action
        title="Unread"
        icon={filter.unread ? Icon.Checkmark : Icon.Circle}
        onAction={() => setFilter({ ...filter, unread: !filter.unread })}
      />
      <Action
        title="Past Your Pace"
        icon={filter.pace ? Icon.Checkmark : Icon.Circle}
        onAction={() => setFilter({ ...filter, pace: !filter.pace })}
      />
      {label && <Action title="Clear Filters" icon={Icon.Eraser} onAction={() => setFilter({})} />}
    </ActionPanel.Submenu>
  );

  // The local search can hide every loaded row; the Filter menu must stay
  // reachable then, since it is the only action left on the screen.
  const hiddenByText =
    data !== undefined &&
    (data.matched ?? data.row_count) > 0 &&
    !narrowedEmpty &&
    [data.your_turn, data.pinned, data.recent, data.quiet].every(
      (rows) => shown(rows).length === 0,
    );
  const item = (row: MessagesRow, band: string) => {
    const topic = row.topics[0];
    const name = row.person?.name ?? row.title;
    const id = rowId(band, row);
    return (
      <List.Item
        key={id}
        id={id}
        icon={senderFace(name, row.person?.addresses?.[0])}
        title={row.title}
        subtitle={pane.showing ? undefined : topic?.subject}
        accessories={
          pane.showing
            ? undefined
            : [
                ...(row.unread ? [UNREAD] : []),
                ...(row.overdue
                  ? [{ tag: { value: "Overdue", color: Color.Orange }, tooltip: row.why }]
                  : []),
                ...badge(row.account_id),
                ...(row.pace_label ? [{ text: row.pace_label }] : []),
                arrival(row.last_at, row.why),
              ]
        }
        detail={
          pane.showing ? (
            <MailPane
              subject={topic?.subject ?? row.title}
              fromName={name}
              fromEmail={row.person?.addresses?.[0]}
              date={row.last_at}
              account={badge.tagOf(row.account_id)}
              unread={row.unread}
              preview={pane.selected === id ? preview : undefined}
              fallback={row.preview?.text ?? row.why}
            >
              {row.why ? <List.Item.Detail.Metadata.Label title="Why Here" text={row.why} /> : null}
              {row.topics.length > 1 ? (
                <List.Item.Detail.Metadata.Label
                  title="Topics"
                  text={row.topics.map((entry) => entry.subject).join(" · ")}
                />
              ) : null}
            </MailPane>
          ) : undefined
        }
        actions={
          <ActionPanel>
            <ActionPanel.Section>
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
                  shortcut={keyFor("messages.reply")}
                  target={
                    <ReplyForm
                      messageId={topic.reply_to_message_id}
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
                  shortcut={keyFor("messages.reply-all")}
                  target={
                    <ReplyForm
                      messageId={topic.reply_to_message_id}
                      kind="reply_all"
                      to={name}
                      onSent={revalidate}
                    />
                  }
                />
              )}
            </ActionPanel.Section>
            <ActionPanel.Section>
              {row.topics.length > 0 && (
                <Action
                  title="Done Here"
                  icon={Icon.Checkmark}
                  shortcut={keyFor("messages.done")}
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
            </ActionPanel.Section>
            <ActionPanel.Section>
              {webBase && (
                <Action.OpenInBrowser
                  title="Open in mxr"
                  url={`${webBase}${web.person(row.id, topic?.thread_id)}`}
                  shortcut={keyFor("messages.as-sent")}
                />
              )}
              <PaneAction pane={pane} />
            </ActionPanel.Section>
            <ActionPanel.Section>{filterMenu}</ActionPanel.Section>
          </ActionPanel>
        }
      />
    );
  };

  return (
    <List
      isLoading={isLoading || search.isLoading}
      isShowingDetail={pane.showing && (topicThreads.size > 0 || search.results.length > 0)}
      onSelectionChange={pane.select}
      navigationTitle={label ? `Messages · ${label}` : (data?.header ?? "Messages")}
      searchBarPlaceholder="Filter people and search all mail"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
      {...search.listProps}
    >
      {hiddenByText && (
        <List.EmptyView
          icon={Icon.MagnifyingGlass}
          title="No people match this search"
          actions={
            <ActionPanel>
              <ActionPanel.Section>{filterMenu}</ActionPanel.Section>
            </ActionPanel>
          }
        />
      )}
      {data && (data.row_count === 0 || narrowedEmpty) && !search.searching && (
        <List.EmptyView
          icon={Icon.Message}
          title={data.empty_state?.split("\n")[0] ?? "No conversations"}
          description={data.empty_state?.split("\n").slice(1).join("\n") || undefined}
          actions={
            label ? (
              <ActionPanel>
                <Action title="Clear Filters" icon={Icon.Eraser} onAction={() => setFilter({})} />
                <ActionPanel.Section>{filterMenu}</ActionPanel.Section>
              </ActionPanel>
            ) : undefined
          }
        />
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
      <SearchSection
        search={search}
        badge={badge}
        pane={pane}
        preview={preview}
        onChange={revalidate}
      />
    </List>
  );
}
