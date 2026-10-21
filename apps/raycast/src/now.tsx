import { Action, ActionPanel, Color, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { getNow, markDone, tickOffTodo } from "./bridge/modes";
import type { NowData } from "./bridge/types";
import { ReplyForm } from "./components/ReplyForm";
import { runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";
import Updates from "./updates";

export default function Now() {
  const { data, isLoading, revalidate } = useCachedPromise(getNow, [], { keepPreviousData: true });
  const webBase = useWebBase();
  const open = (path: string) =>
    webBase && (
      <Action.OpenInBrowser
        title="Open in mxr"
        url={`${webBase}${path}`}
        shortcut={Keyboard.Shortcut.Common.Open}
      />
    );
  const now: NowData | undefined = data;

  return (
    <List
      isLoading={isLoading}
      navigationTitle={now?.header ?? "Now"}
      searchBarPlaceholder="Filter Now"
    >
      {now && now.item_count === 0 && (
        <List.EmptyView
          icon={Icon.CheckCircle}
          title={now.empty_state ?? "Nothing needs you"}
          description={now.not_now ?? undefined}
        />
      )}
      <List.Section title="People" subtitle={now?.headline}>
        {now?.people.rows.map(({ row, why }) => (
          <List.Item
            key={`person-${row.thread_id}`}
            icon={row.overdue ? { source: Icon.Clock, tintColor: Color.Orange } : Icon.Person}
            title={row.counterparty_name ?? row.counterparty_email}
            subtitle={row.subject}
            accessories={[{ text: why }, { date: new Date(row.since) }]}
            actions={
              <ActionPanel>
                <Action.Push
                  title="Reply"
                  icon={Icon.Reply}
                  target={
                    <ReplyForm
                      messageId={row.message_id}
                      threadId={row.thread_id}
                      to={row.counterparty_name ?? row.counterparty_email}
                      onSent={revalidate}
                    />
                  }
                />
                {open(web.thread("messages", row.thread_id))}
                <Action
                  title="Done Here"
                  icon={Icon.Checkmark}
                  shortcut={Keyboard.Shortcut.Common.Edit}
                  onAction={() =>
                    runPreviewed({
                      title: "Done here?",
                      confirm: "Done Here",
                      preview: () => markDone("messages", [row.thread_id], true),
                      commit: () => markDone("messages", [row.thread_id], false),
                      describe: (done) => done.items.map((item) => item.copy).join("\n"),
                      undoId: (done) => done.mutation_id,
                      done: "Done here",
                      refresh: revalidate,
                    })
                  }
                />
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
      <List.Section title="Due Soon" subtitle={now?.due_soon.more_line ?? undefined}>
        {now?.due_soon.todos.map(({ todo, why }) => (
          <List.Item
            key={`todo-${todo.id}`}
            icon={todo.overdue ? { source: Icon.Alarm, tintColor: Color.Red } : Icon.Circle}
            title={todo.title}
            subtitle={todo.when_label ?? undefined}
            accessories={[{ text: why }]}
            actions={
              <ActionPanel>
                {open(web.todo(todo.id))}
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
                      done: "Ticked off",
                      refresh: revalidate,
                    })
                  }
                />
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
      {now?.updates && (
        <List.Section title="Updates">
          <List.Item
            icon={Icon.Bell}
            title={now.updates.headline ?? now.updates.title ?? "Updates"}
            subtitle={now.updates.line}
            accessories={now.updates.cut_label ? [{ text: now.updates.cut_label }] : []}
            actions={
              <ActionPanel>
                <Action.Push title="Show Updates" icon={Icon.List} target={<Updates />} />
                {open(web.mode("updates"))}
              </ActionPanel>
            }
          />
        </List.Section>
      )}
      {now?.reading && (
        <List.Section title="Reading">
          <List.Item
            icon={Icon.Book}
            title={now.reading.subject}
            subtitle={now.reading.sender_name ?? now.reading.sender_email}
            accessories={[{ text: now.reading.why }]}
            actions={
              <ActionPanel>{open(web.thread("reading", now.reading.thread_id))}</ActionPanel>
            }
          />
        </List.Section>
      )}
      {now?.coming_up && now.coming_up.length > 0 && (
        <List.Section title="Coming Up">
          {now.coming_up.map((moment) => (
            <List.Item
              key={`moment-${moment.record_id}-${moment.at}`}
              icon={Icon.Calendar}
              title={moment.label}
              accessories={[{ date: new Date(moment.at) }]}
              actions={<ActionPanel>{open(web.mode("archive"))}</ActionPanel>}
            />
          ))}
        </List.Section>
      )}
    </List>
  );
}
