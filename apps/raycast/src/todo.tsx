import { Action, ActionPanel, Color, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { getTodo, tickOffTodo } from "./bridge/modes";
import type { Todo } from "./bridge/types";
import { runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";

export default function TodoCommand() {
  const { data, isLoading, revalidate } = useCachedPromise(getTodo, [], { keepPreviousData: true });
  const webBase = useWebBase();

  const item = (todo: Todo) => (
    <List.Item
      key={todo.id}
      icon={todo.overdue ? { source: Icon.Alarm, tintColor: Color.Red } : Icon.Circle}
      title={todo.title}
      subtitle={todo.counterparty ?? undefined}
      accessories={[{ text: todo.when_label ?? undefined, tooltip: todo.why ?? undefined }]}
      actions={
        <ActionPanel>
          <Action
            title="Tick Off"
            icon={Icon.CheckCircle}
            shortcut={Keyboard.Shortcut.Common.Edit}
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
              shortcut={Keyboard.Shortcut.Common.Open}
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

  return (
    <List
      isLoading={isLoading}
      navigationTitle={data?.header ?? "To Do"}
      searchBarPlaceholder="Filter to-dos"
    >
      {data?.empty_state && <List.EmptyView icon={Icon.CheckCircle} title={data.empty_state} />}
      <List.Section title="Now" subtitle={data?.headline}>
        {data?.now.map(item)}
      </List.Section>
      {data?.coming_up.map((week) => (
        <List.Section key={week.week_start} title={week.label}>
          {week.todos.map(item)}
        </List.Section>
      ))}
      <List.Section title="Whenever">{data?.whenever.map(item)}</List.Section>
      <List.Section title="Later">{data?.later.map(item)}</List.Section>
    </List>
  );
}
