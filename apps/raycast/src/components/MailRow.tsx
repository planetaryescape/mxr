import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import type { AccountTag, useAccountBadge } from "./accountBadge";
import { senderFace } from "./faces";
import { MailPane, type Pane, PaneAction, type Preview, searchRowId } from "./pane";
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

/** An unread row's mark: a small dot, since the icon is the sender's initials. */
export const UNREAD: List.Item.Accessory = {
  icon: { source: Icon.CircleFilled, tintColor: Color.Blue },
  tooltip: "Unread",
};

/** A conversation from the inbox or a search: Read, Reply, Open in mxr. */
export function MailRowItem({
  row,
  id,
  webPath,
  badge = [],
  account,
  pane,
  preview,
  onChange,
}: {
  row: MailRow;
  /** The list item's id, for the pane's selection; the thread by default. */
  id?: string;
  /** The account tag, from `useAccountBadge`. */
  badge?: List.Item.Accessory[];
  /** The account, named in the reading pane. */
  account?: AccountTag;
  pane?: Pane;
  /** The selected row's conversation, while the pane shows. */
  preview?: Preview;
  /** Where Open in mxr goes; the search page by default. */
  webPath?: string;
  onChange?: () => void;
}) {
  const webBase = useWebBase();
  const path = webPath ?? web.search(row.thread_id);
  const count = row.message_count ?? 1;
  const itemId = id ?? row.thread_id;
  return (
    <List.Item
      key={row.thread_id}
      id={itemId}
      icon={senderFace(row.sender, row.sender_detail)}
      title={row.sender}
      subtitle={pane?.showing ? undefined : row.subject}
      keywords={[row.subject, row.sender_detail ?? "", row.snippet]}
      accessories={
        pane?.showing
          ? undefined
          : [
              ...(row.unread ? [UNREAD] : []),
              ...badge,
              ...(row.starred
                ? [{ icon: { source: Icon.Star, tintColor: Color.Yellow }, tooltip: "Starred" }]
                : []),
              ...(row.has_attachments ? [{ icon: Icon.Paperclip, tooltip: "Attachments" }] : []),
              ...(count > 1 ? [{ text: String(count), tooltip: `${count} messages` }] : []),
              arrival(row.date),
            ]
      }
      detail={
        pane?.showing ? (
          <MailPane
            subject={row.subject}
            fromName={row.sender}
            fromEmail={row.sender_detail}
            date={row.date}
            account={account}
            unread={row.unread}
            preview={pane.selected === itemId ? preview : undefined}
            fallback={row.snippet}
          />
        ) : undefined
      }
      actions={
        <ActionPanel>
          <ActionPanel.Section>
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
          </ActionPanel.Section>
          <ActionPanel.Section>
            {webBase && (
              <Action.OpenInBrowser
                title="Open in mxr"
                url={`${webBase}${path}`}
                shortcut={keyFor("list.open")}
              />
            )}
            {pane ? <PaneAction pane={pane} /> : null}
          </ActionPanel.Section>
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
  pane,
  preview,
  onChange,
}: {
  search: ReturnType<typeof useMailSearch>;
  badge: ReturnType<typeof useAccountBadge>;
  pane?: Pane;
  preview?: Preview;
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
          key={searchRowId(row.thread_id)}
          id={searchRowId(row.thread_id)}
          row={row}
          badge={badge(row.account_id)}
          account={badge.tagOf(row.account_id)}
          pane={pane}
          preview={preview}
          onChange={onChange}
        />
      ))}
    </List.Section>
  );
}
