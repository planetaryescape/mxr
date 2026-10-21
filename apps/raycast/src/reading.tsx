import { Action, ActionPanel, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { getReading, markDone, saveForLater } from "./bridge/modes";
import type { ReadingItem } from "./bridge/types";
import { itemErrors, runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";

export default function Reading() {
  const { data, isLoading, revalidate } = useCachedPromise(getReading, [], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();

  const item = (entry: ReadingItem, band: string) => (
    <List.Item
      key={`${band}-${entry.item_key}`}
      icon={entry.on_later ? Icon.Bookmark : Icon.Book}
      title={entry.title}
      subtitle={entry.source}
      accessories={[
        ...(entry.fades ? [{ text: entry.fades }] : []),
        { text: `${entry.minutes} min`, tooltip: entry.why },
      ]}
      actions={
        <ActionPanel>
          {webBase && (
            <Action.OpenInBrowser
              title="Read in mxr"
              url={`${webBase}${web.readingItem(entry.item_key)}`}
              shortcut={Keyboard.Shortcut.Common.Open}
            />
          )}
          {entry.url && <Action.OpenInBrowser title="Open Original" url={entry.url} />}
          {!entry.on_later && (
            <Action
              title="Save for Later"
              icon={Icon.Bookmark}
              shortcut={{ modifiers: ["cmd"], key: "l" }}
              onAction={() =>
                runPreviewed({
                  title: "Save for later?",
                  confirm: "Save",
                  preview: () => saveForLater(entry.item_key, true),
                  commit: () => saveForLater(entry.item_key, false),
                  describe: () => `${entry.title} stays on your Later shelf instead of fading.`,
                  failures: (later) => itemErrors(later.items),
                  done: "Saved for later",
                  refresh: revalidate,
                })
              }
            />
          )}
          <Action
            title="Let Go"
            icon={Icon.Checkmark}
            shortcut={Keyboard.Shortcut.Common.Edit}
            onAction={() =>
              runPreviewed({
                title: "Let this go?",
                confirm: "Let Go",
                preview: () => markDone("reading", [entry.thread_id], true),
                commit: () => markDone("reading", [entry.thread_id], false),
                describe: (done) => done.items.map((outcome) => outcome.copy).join("\n"),
                failures: (done) => itemErrors(done.items),
                undoId: (done) => done.mutation_id,
                done: "Let go",
                refresh: revalidate,
              })
            }
          />
        </ActionPanel>
      }
    />
  );

  return (
    <List
      isLoading={isLoading}
      navigationTitle={data?.header ?? "Reading"}
      searchBarPlaceholder="Filter reading"
    >
      {data?.empty && <List.EmptyView icon={Icon.Book} title={data.empty.line} />}
      {data?.bands.map((group) => (
        <List.Section key={group.band} title={group.label} subtitle={group.note ?? undefined}>
          {group.items.map((entry) => item(entry, group.band))}
        </List.Section>
      ))}
      <List.Section title="Later" subtitle={data ? String(data.later_count) : undefined}>
        {data?.later.map((entry) => item(entry, "later"))}
      </List.Section>
    </List>
  );
}
