import { Action, ActionPanel, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getRecords } from "./bridge/modes";
import { AccountDropdown } from "./components/AccountDropdown";
import { useLiveRefresh } from "./components/live";
import { SearchSection, useMailSearch } from "./components/MailRow";
import { ReadAction } from "./components/ThreadDetail";
import { arrival } from "./components/time";
import { useWebBase, web } from "./components/web";

export default function Archive() {
  const [account, setAccount] = useState<string>();
  const { data, isLoading, revalidate } = useCachedPromise(getRecords, [account], {
    keepPreviousData: true,
  });
  useLiveRefresh(revalidate);
  const webBase = useWebBase();
  const search = useMailSearch(account);

  return (
    <List
      isLoading={isLoading || search.isLoading}
      navigationTitle={data?.header ?? "Archive"}
      searchBarPlaceholder="Filter records and search all mail"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
      {...search.listProps}
    >
      {data?.empty_state && !search.searching && (
        <List.EmptyView icon={Icon.Box} title={data.empty_state} />
      )}
      <List.Section
        title="Records"
        subtitle={data ? `${data.matching} of ${data.total}` : undefined}
      >
        {data?.records
          .filter((record) =>
            search.keep(record.issuer, record.kind_label, record.detail_line, record.reference),
          )
          .map((record) => (
            <List.Item
              key={record.id}
              icon={Icon.Document}
              title={record.issuer ?? record.kind_label}
              subtitle={record.detail_line ?? record.reference ?? undefined}
              accessories={[
                { tag: record.kind_label },
                ...(record.date ? [arrival(record.date)] : []),
              ]}
              actions={
                <ActionPanel>
                  {record.thread_id && (
                    <ReadAction
                      title="Read Source Email"
                      threadId={record.thread_id}
                      webPath={web.thread("archive", record.thread_id)}
                    />
                  )}
                  {webBase && (
                    <Action.OpenInBrowser
                      title="Open in mxr"
                      url={`${webBase}${web.mode("archive")}`}
                      shortcut={Keyboard.Shortcut.Common.Open}
                    />
                  )}
                  {record.reference && (
                    <Action.CopyToClipboard title="Copy Reference" content={record.reference} />
                  )}
                </ActionPanel>
              }
            />
          ))}
      </List.Section>
      <SearchSection search={search} onChange={revalidate} />
    </List>
  );
}
