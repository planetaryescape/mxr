import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import { getProgressIcon, useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getReading, markDone, saveForLater } from "./bridge/modes";
import type { ReadingItem } from "./bridge/types";
import { AccountDropdown } from "./components/AccountDropdown";
import { useAccountBadge } from "./components/accountBadge";
import { useLiveRefresh } from "./components/live";
import { SearchSection, useMailSearch } from "./components/MailRow";
import { senderFace } from "./components/faces";
import { MailPane, PaneAction, searchRowThread, usePane, usePreview } from "./components/pane";
import { ReadAction } from "./components/ThreadDetail";
import { itemErrors, runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";
import { keyFor } from "./components/keys";

const PROGRESS_BLUE = "#3b82f6";

export default function Reading() {
  const [account, setAccount] = useState<string>();
  const { data, isLoading, revalidate } = useCachedPromise(getReading, [account], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();
  const search = useMailSearch(account);
  const shown = (items: ReadingItem[] | undefined) =>
    (items ?? []).filter((entry) => search.keep(entry.title, entry.source));

  const pane = usePane("reading");
  const threads = new Map<string, string>([
    ...(data?.bands ?? []).flatMap((group) =>
      group.items.map((entry): [string, string] => [
        `${group.band}-${entry.item_key}`,
        entry.thread_id,
      ]),
    ),
    ...(data?.later ?? []).map((entry): [string, string] => [
      `later-${entry.item_key}`,
      entry.thread_id,
    ]),
  ]);
  const selectedThread =
    searchRowThread(pane.selected) ?? (pane.selected ? threads.get(pane.selected) : undefined);
  const preview = usePreview(pane.showing ? selectedThread : undefined);

  const item = (entry: ReadingItem, band: string) => (
    <List.Item
      key={`${band}-${entry.item_key}`}
      id={`${band}-${entry.item_key}`}
      icon={senderFace(entry.source, entry.sender_email)}
      title={entry.title}
      subtitle={pane.showing ? undefined : entry.source}
      accessories={
        pane.showing
          ? undefined
          : [
              ...(entry.on_later
                ? [
                    {
                      icon: { source: Icon.Bookmark, tintColor: Color.Purple },
                      tooltip: "On Later",
                    },
                  ]
                : []),
              ...(entry.progress > 0 && entry.progress < 1
                ? [
                    {
                      icon: getProgressIcon(entry.progress, PROGRESS_BLUE),
                      tooltip: `${Math.round(entry.progress * 100)}% read`,
                    },
                  ]
                : []),
              ...badge(entry.account_id),
              ...(entry.fades ? [{ text: entry.fades }] : []),
              { text: `${entry.minutes} min`, tooltip: entry.why },
            ]
      }
      detail={
        pane.showing ? (
          <MailPane
            subject={entry.title}
            fromName={entry.source}
            fromEmail={entry.sender_email}
            date={entry.arrived_at}
            account={badge.tagOf(entry.account_id)}
            preview={pane.selected === `${band}-${entry.item_key}` ? preview : undefined}
            fallback={entry.standfirst ?? entry.why}
          >
            <List.Item.Detail.Metadata.Label title="Reading Time" text={`${entry.minutes} min`} />
            {entry.progress > 0 && entry.progress < 1 ? (
              <List.Item.Detail.Metadata.Label
                title="Progress"
                text={`${Math.round(entry.progress * 100)}% read`}
              />
            ) : null}
            {entry.fades ? (
              <List.Item.Detail.Metadata.Label title="Fades" text={entry.fades} />
            ) : null}
          </MailPane>
        ) : undefined
      }
      actions={
        <ActionPanel>
          <ActionPanel.Section>
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
          </ActionPanel.Section>
          <ActionPanel.Section>
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
          </ActionPanel.Section>
          <ActionPanel.Section>
            <PaneAction pane={pane} />
          </ActionPanel.Section>
        </ActionPanel>
      }
    />
  );
  const badge = useAccountBadge(account);
  useLiveRefresh(revalidate);

  return (
    <List
      isLoading={isLoading || search.isLoading}
      isShowingDetail={pane.showing && (threads.size > 0 || search.results.length > 0)}
      onSelectionChange={pane.select}
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
