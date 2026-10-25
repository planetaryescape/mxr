import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import type { useAccountBadge } from "./accountBadge";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { type MailRow, searchMail } from "../bridge/mail";
import { ReplyForm } from "./ReplyForm";
import { ReadAction } from "./ThreadDetail";
import { arrival } from "./time";
import { useWebBase, web } from "./web";
import { keyFor } from "./keys";

/** Shorter queries match too much of the mailbox to be worth asking. */
const MIN_QUERY = 2;

/** A conversation from the inbox or a search: Read, Reply, Open in mxr. */
export function MailRowItem({
  row,
  webPath,
  badge = [],
  onChange,
}: {
  row: MailRow;
  /** The account tag, from `useAccountBadge`. */
  badge?: List.Item.Accessory[];
  /** Where Open in mxr goes; the search page by default. */
  webPath?: string;
  onChange?: () => void;
}) {
  const webBase = useWebBase();
  const path = webPath ?? web.search(row.thread_id);
  const count = row.message_count ?? 1;
  return (
    <List.Item
      key={row.thread_id}
      icon={row.unread ? { source: Icon.CircleFilled, tintColor: Color.Blue } : Icon.Circle}
      title={row.sender}
      subtitle={row.subject}
      keywords={[row.sender_detail ?? "", row.snippet]}
      accessories={[
        ...badge,
        ...(row.starred
          ? [{ icon: { source: Icon.Star, tintColor: Color.Yellow }, tooltip: "Starred" }]
          : []),
        ...(row.has_attachments ? [{ icon: Icon.Paperclip, tooltip: "Attachments" }] : []),
        ...(count > 1 ? [{ text: String(count), tooltip: `${count} messages` }] : []),
        arrival(row.date),
      ]}
      actions={
        <ActionPanel>
          <ReadAction
            threadId={row.thread_id}
            webPath={path}
            replyTo={{ messageId: row.id, to: row.sender }}
            onChange={onChange}
          />
          <Action.Push
            title="Reply"
            icon={Icon.Reply}
            shortcut={keyFor("mail.reply")}
            target={<ReplyForm messageId={row.id} to={row.sender} onSent={onChange} />}
          />
          {webBase && (
            <Action.OpenInBrowser
              title="Open in mxr"
              url={`${webBase}${path}`}
              shortcut={keyFor("list.open")}
            />
          )}
        </ActionPanel>
      }
    />
  );
}

/**
 * The search bar as an mxr search. Raycast's own filter only sees the rows
 * a command already loaded, so a mode list hands the text here instead:
 * `keep` filters the mode's rows, and `results` is every matching
 * conversation in the mailbox, from mxr's index.
 */
export function useMailSearch(account: string | undefined) {
  const [query, setQuery] = useState("");
  const q = query.trim();
  const searching = q.length >= MIN_QUERY;
  const { data, isLoading } = useCachedPromise(
    (text: string, scope: string | undefined) => searchMail(text, scope, 0),
    [q, account],
    { execute: searching, keepPreviousData: true },
  );
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  return {
    searching,
    isLoading: searching && isLoading,
    /** For the List: Raycast stops filtering and throttles the text. */
    listProps: { onSearchTextChange: setQuery, throttle: true, filtering: false } as const,
    /** Whether a mode row matches what is typed. */
    keep: (...fields: (string | null | undefined)[]) => {
      if (words.length === 0) return true;
      const haystack = fields.join(" ").toLowerCase();
      return words.every((word) => haystack.includes(word));
    },
    results: searching ? (data?.rows ?? []) : [],
    total: searching ? data?.total : undefined,
  };
}

/** "All Mail": what mxr's search found, below a mode's own matches. */
export function SearchSection({
  search,
  badge,
  onChange,
}: {
  search: ReturnType<typeof useMailSearch>;
  badge: ReturnType<typeof useAccountBadge>;
  onChange?: () => void;
}) {
  if (!search.searching) return null;
  return (
    <List.Section
      title="All Mail"
      subtitle={search.total === undefined ? undefined : `${search.total} found`}
    >
      {search.results.map((row) => (
        <MailRowItem
          key={`search-${row.thread_id}`}
          row={row}
          badge={badge(row.account_id)}
          onChange={onChange}
        />
      ))}
    </List.Section>
  );
}
