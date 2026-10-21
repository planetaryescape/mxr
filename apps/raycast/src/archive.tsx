import { Action, ActionPanel, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { getRecords } from "./bridge/modes";
import { useWebBase, web } from "./components/web";

export default function Archive() {
  const { data, isLoading } = useCachedPromise(getRecords, [], { keepPreviousData: true });
  const webBase = useWebBase();

  return (
    <List
      isLoading={isLoading}
      navigationTitle={data?.header ?? "Archive"}
      searchBarPlaceholder="Filter records"
    >
      {data?.empty_state && <List.EmptyView icon={Icon.Box} title={data.empty_state} />}
      <List.Section
        title="Records"
        subtitle={data ? `${data.matching} of ${data.total}` : undefined}
      >
        {data?.records.map((record) => (
          <List.Item
            key={record.id}
            icon={Icon.Document}
            title={record.issuer ?? record.kind_label}
            subtitle={record.detail_line ?? record.reference ?? undefined}
            accessories={[
              { tag: record.kind_label },
              ...(record.date ? [{ date: new Date(record.date) }] : []),
            ]}
            actions={
              <ActionPanel>
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
    </List>
  );
}
