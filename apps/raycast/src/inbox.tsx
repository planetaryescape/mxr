import { Icon, List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { PAGE_SIZE, getInbox, searchMail } from "./bridge/mail";
import { AccountDropdown } from "./components/AccountDropdown";
import { useLiveRefresh } from "./components/live";
import { MailRowItem } from "./components/MailRow";
import { web } from "./components/web";

/**
 * The whole inbox, newest first, a page at a time. Typing searches all
 * mail through mxr's index and query language instead.
 */
export default function Inbox() {
  const [account, setAccount] = useState<string>();
  const [query, setQuery] = useState("");
  const q = query.trim();

  const { data, isLoading, pagination, revalidate } = useCachedPromise(
    (text: string, scope: string | undefined) =>
      async ({ cursor }: { page: number; cursor?: number }) => {
        const offset = cursor ?? 0;
        const result = text ? await searchMail(text, scope, offset) : await getInbox(scope, offset);
        return {
          data: result.rows,
          hasMore: result.nextOffset !== undefined,
          cursor: result.nextOffset,
        };
      },
    [q, account],
    { keepPreviousData: true },
  );
  // A conversation can land on two pages when mail arrives between them.
  const rows = [...new Map((data ?? []).map((row) => [row.thread_id, row])).values()];
  useLiveRefresh(revalidate, (data?.length ?? 0) <= PAGE_SIZE);

  return (
    <List
      isLoading={isLoading}
      pagination={pagination}
      onSearchTextChange={setQuery}
      throttle
      filtering={false}
      searchBarPlaceholder="Search mail: from:ada is:unread has:attachment…"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
    >
      {!isLoading && rows.length === 0 && (
        <List.EmptyView
          icon={q ? Icon.MagnifyingGlass : Icon.Tray}
          title={q ? "No mail matches" : "Inbox is empty"}
          description={q ? "mxr searched every message in this account scope." : undefined}
        />
      )}
      <List.Section title={q ? "Search Results" : "Inbox"}>
        {rows.map((row) => (
          <MailRowItem
            key={row.thread_id}
            row={row}
            webPath={q ? web.search(row.thread_id) : web.inbox(row.thread_id)}
            onChange={revalidate}
          />
        ))}
      </List.Section>
    </List>
  );
}
