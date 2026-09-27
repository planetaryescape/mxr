import { useInfiniteQuery, useQuery } from "@tanstack/react-query";

import { fetchMailbox, fetchShell, mailboxKey, shellKey } from "./api";
import type { MailLens } from "./lenses";
import type { MailboxResponse, MessageGroupView } from "./types";
import { useUiPrefs } from "@/state/uiPrefsStore";

const MAILBOX_PAGE_SIZE = 150;

export function useShellQuery() {
  return useQuery({ queryKey: shellKey, queryFn: fetchShell, staleTime: 30_000 });
}

/**
 * Paged rows for one lens, honouring the account scope and list mode. The
 * cache keeps TanStack's `{pages, pageParams}` shape; optimistic changes are
 * projected at render time (see pendingMailOps), never written into it.
 */
export function useMailboxQuery(lens: MailLens | null) {
  const account = useUiPrefs((s) => s.accountScope);
  const view = useUiPrefs((s) => s.listMode);
  const params = lens ? { ...lens.params, view, limit: MAILBOX_PAGE_SIZE, account } : null;
  return useInfiniteQuery({
    queryKey: mailboxKey(params ?? { lens_kind: "inbox" }),
    queryFn: ({ pageParam }) =>
      fetchMailbox({ ...(params ?? { lens_kind: "inbox" }), offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (lastPage) =>
      lastPage.mailbox.has_more && typeof lastPage.mailbox.next_offset === "number"
        ? lastPage.mailbox.next_offset
        : undefined,
    select: (data) => mergeMailboxPages(data.pages, view),
    enabled: params !== null,
    staleTime: 10_000,
    placeholderData: (previous, previousQuery) =>
      // Keep rows on screen while the mode or scope changes within a lens.
      previousQuery?.queryKey[1] &&
      (previousQuery.queryKey[1] as { lens_kind?: string; label_id?: string }).lens_kind ===
        params?.lens_kind &&
      (previousQuery.queryKey[1] as { label_id?: string }).label_id === params?.label_id
        ? previous
        : undefined,
  });
}

/**
 * Join pages into one grouped list. Thread views dedupe by thread: a busy
 * conversation can surface on two pages as new mail shifts the offsets.
 */
export function mergeMailboxPages(
  pages: MailboxResponse[],
  view: "threads" | "messages",
): MailboxResponse | undefined {
  const first = pages[0];
  if (!first) return undefined;
  const groups: MessageGroupView[] = [];
  const groupIndexes = new Map<string, number>();
  const seen = new Set<string>();

  for (const page of pages) {
    for (const group of page.mailbox.groups) {
      const rows = group.rows.filter((row) => {
        const key = view === "threads" ? row.thread_id || row.id : row.id;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
      if (rows.length === 0) continue;
      const existing = groupIndexes.get(group.id);
      if (existing === undefined) {
        groupIndexes.set(group.id, groups.length);
        groups.push({ ...group, rows });
      } else {
        const current = groups[existing]!;
        groups[existing] = { ...current, rows: [...current.rows, ...rows] };
      }
    }
  }
  const last = pages.at(-1) ?? first;
  return {
    ...first,
    mailbox: {
      ...first.mailbox,
      has_more: last.mailbox.has_more,
      next_offset: last.mailbox.next_offset,
      groups,
    },
  };
}
