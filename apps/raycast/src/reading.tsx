import { Action, ActionPanel, Icon, List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getReading, markDone, saveForLater } from "./bridge/modes";
import type { ReadingItem } from "./bridge/types";
import { AccountDropdown } from "./components/AccountDropdown";
import { useAccountBadge } from "./components/accountBadge";
import { useLiveRefresh } from "./components/live";
import { SearchSection, useMailSearch } from "./components/MailRow";
import { ReadAction } from "./components/ThreadDetail";
import { itemErrors, runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";
import { keyFor } from "./components/keys";

export default function Reading() {
  const [account, setAccount] = useState<string>();
  const { data, isLoading, revalidate } = useCachedPromise(getReading, [account], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();
  const search = useMailSearch(account);
  const shown = (items: ReadingItem[] | undefined) =>
    (items ?? []).filter((entry) => search.keep(entry.title, entry.source));

  const item = (entry: ReadingItem, band: string) => (
    <List.Item
      key={`${band}-${entry.item_key}`}
      icon={entry.on_later ? Icon.Bookmark : Icon.Book}
      title={entry.title}
      subtitle={entry.source}
      accessories={[
        ...badge(entry.account_id),
        ...(entry.fades ? [{ text: entry.fades }] : []),
        { text: `${entry.minutes} min`, tooltip: entry.why },
      ]}
      actions={
        <ActionPanel>
          <ReadAction
            threadId={entry.thread_id}
            webPath={web.readingItem(entry.item_key)}
            onChange={revalidate}
          />
          {webBase && (
            <Action.OpenInBrowser
              title="Read in mxr"
              url={`${webBase}${web.readingItem(entry.item_key)}`}
              shortcut={keyFor("reading.open-email")}
            />
          )}
          {entry.url && <Action.OpenInBrowser title="Open Original" url={entry.url} />}
          {!entry.on_later && (
            <Action
              title="Save for Later"
              icon={Icon.Bookmark}
              shortcut={keyFor("reading.later")}
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
            shortcut={keyFor("reading.let-go")}
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
  const badge = useAccountBadge(account);
  useLiveRefresh(revalidate);

  return (
    <List
      isLoading={isLoading || search.isLoading}
      navigationTitle={data?.header ?? "Reading"}
      searchBarPlaceholder="Filter reading and search all mail"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
      {...search.listProps}
    >
      {data?.empty && !search.searching && (
        <List.EmptyView icon={Icon.Book} title={data.empty.line} />
      )}
      {data?.bands.map((group) => (
        <List.Section key={group.band} title={group.label} subtitle={group.note ?? undefined}>
          {shown(group.items).map((entry) => item(entry, group.band))}
        </List.Section>
      ))}
      <List.Section title="Later" subtitle={data ? String(data.later_count) : undefined}>
        {shown(data?.later).map((entry) => item(entry, "later"))}
      </List.Section>
      <SearchSection search={search} badge={badge} onChange={revalidate} />
    </List>
  );
}
