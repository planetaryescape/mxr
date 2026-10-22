import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getTodo, tickOffTodo } from "./bridge/modes";
import type { Todo } from "./bridge/types";
import { AccountDropdown } from "./components/AccountDropdown";
import { useAccountBadge } from "./components/accountBadge";
import { useLiveRefresh } from "./components/live";
import { SearchSection, useMailSearch } from "./components/MailRow";
import { ReadAction } from "./components/ThreadDetail";
import { runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";
import { keyFor } from "./components/keys";

export default function TodoCommand() {
  const [account, setAccount] = useState<string>();
  const { data, isLoading, revalidate } = useCachedPromise(getTodo, [account], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();
  const search = useMailSearch(account);
  const shown = (todos: Todo[] | undefined) =>
    (todos ?? []).filter((todo) => search.keep(todo.title, todo.counterparty, todo.when_label));

  const item = (todo: Todo) => (
    <List.Item
      key={todo.id}
      icon={todo.overdue ? { source: Icon.Alarm, tintColor: Color.Red } : Icon.Circle}
      title={todo.title}
      subtitle={todo.counterparty ?? undefined}
      accessories={[
        ...badge(todo.account_id),
        { text: todo.when_label ?? undefined, tooltip: todo.why ?? undefined },
      ]}
      actions={
        <ActionPanel>
          <Action
            title="Tick Off"
            icon={Icon.CheckCircle}
            shortcut={keyFor("todo.done")}
            onAction={() =>
              runPreviewed({
                title: "Tick this off?",
                confirm: "Tick Off",
                preview: () => tickOffTodo(todo.id, true),
                commit: () => tickOffTodo(todo.id, false),
                describe: (change) => change.change.summary,
                failures: (change) =>
                  (change.change.unchanged ?? []).map(() => "Already done, or no longer a to-do."),
                done: "Ticked off",
                refresh: revalidate,
              })
            }
          />
          {webBase && (
            <Action.OpenInBrowser
              title="Open in mxr"
              url={`${webBase}${web.todo(todo.id)}`}
              shortcut={keyFor("todo.open-email")}
            />
          )}
          {todo.thread_id && (
            <ReadAction
              title="Read Source Email"
              threadId={todo.thread_id}
              webPath={web.thread("todo", todo.thread_id)}
              onChange={revalidate}
            />
          )}
          {webBase && todo.thread_id && (
            <Action.OpenInBrowser
              title="Open Source Email in mxr"
              url={`${webBase}${web.thread("todo", todo.thread_id)}`}
            />
          )}
        </ActionPanel>
      }
    />
  );
  const badge = useAccountBadge(account);
  useLiveRefresh(revalidate);

  return (
    <List
      isLoading={isLoading || search.isLoading}
      navigationTitle={data?.header ?? "To Do"}
      searchBarPlaceholder="Filter to-dos and search all mail"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
      {...search.listProps}
    >
      {data?.empty_state && !search.searching && (
        <List.EmptyView icon={Icon.CheckCircle} title={data.empty_state} />
      )}
      <List.Section title="Now" subtitle={data?.headline}>
        {shown(data?.now).map(item)}
      </List.Section>
      {data?.coming_up.map((week) => (
        <List.Section key={week.week_start} title={week.label}>
          {shown(week.todos).map(item)}
        </List.Section>
      ))}
      <List.Section title="Whenever">{shown(data?.whenever).map(item)}</List.Section>
      <List.Section title="Later">{shown(data?.later).map(item)}</List.Section>
      <SearchSection search={search} badge={badge} onChange={revalidate} />
    </List>
  );
}
