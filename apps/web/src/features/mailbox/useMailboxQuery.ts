import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";

import { fetchMailbox, fetchShell, mailboxKey, shellKey, type MailboxLensParams } from "./api";
import type { MailboxResponse, MessageGroupView, ShellResponse, SidebarItem } from "./types";

const MAILBOX_PAGE_SIZE = 200;

function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function itemsFromShell(shell?: ShellResponse): SidebarItem[] {
  return shell?.sidebar?.sections?.flatMap((section) => section.items) ?? [];
}

function lensFromItem(item: SidebarItem): MailboxLensParams | undefined {
  const lens = item.lens;
  if (!lens) return undefined;
  if (lens.kind === "label" && lens.labelId) {
    return { lens_kind: "label", label_id: lens.labelId };
  }
  if (lens.kind === "saved_search" && lens.savedSearch) {
    return { lens_kind: "saved_search", saved_search: lens.savedSearch };
  }
  if (lens.kind === "subscription" && lens.senderEmail) {
    return { lens_kind: "subscription", sender_email: lens.senderEmail };
  }
  if (lens.kind === "all_mail") return { lens_kind: "all_mail" };
  if (lens.kind === "inbox") return { lens_kind: "inbox" };
  return undefined;
}

export function resolveMailboxLens(pathname: string, shell?: ShellResponse): MailboxLensParams {
  const items = itemsFromShell(shell);
  const parts = pathname.split("/").filter(Boolean).map(decodeURIComponent);
  if (parts[0] !== "m") return { lens_kind: "inbox" };

  if (parts[1] === "label" && parts[2]) {
    const target = parts[2];
    const item = items.find(
      (candidate) => candidate.id === target || slugify(candidate.label) === target,
    );
    return lensFromItem(item ?? ({} as SidebarItem)) ?? { lens_kind: "all_mail" };
  }

  if (parts[1] === "saved" && parts[2]) {
    const target = parts[2];
    const item = items.find(
      (candidate) =>
        candidate.id === `saved-search-${target}` || slugify(candidate.label) === target,
    );
    return lensFromItem(item ?? ({} as SidebarItem)) ?? { lens_kind: "inbox" };
  }

  const mailbox = parts[1] ?? "inbox";
  if (mailbox === "inbox") return { lens_kind: "inbox" };
  if (mailbox === "archive" || mailbox === "all-mail") return { lens_kind: "all_mail" };
  const item = items.find(
    (candidate) => slugify(candidate.label) === mailbox || candidate.id === mailbox,
  );
  return lensFromItem(item ?? ({} as SidebarItem)) ?? { lens_kind: "all_mail" };
}

export function useShellQuery() {
  return useQuery({ queryKey: shellKey, queryFn: fetchShell, staleTime: 30_000 });
}

export function useMailboxQuery() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const shell = useShellQuery();
  const lens = resolveMailboxLens(pathname, shell.data);
  return useInfiniteQuery({
    queryKey: mailboxKey({ ...lens, view: "threads", limit: MAILBOX_PAGE_SIZE }),
    queryFn: ({ pageParam }) =>
      fetchMailbox({ ...lens, view: "threads", limit: MAILBOX_PAGE_SIZE, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) => {
      // Saved-search runs can't paginate: Request::RunSavedSearch takes no
      // offset. A subscription drilldown (sender_email set) runs through
      // Request::Search, which does — the bridge reports has_more for it.
      if (lens.lens_kind === "saved_search") return undefined;
      if (lens.lens_kind === "subscription" && !lens.sender_email) return undefined;
      if (lastPage.mailbox.has_more && typeof lastPage.mailbox.next_offset === "number") {
        return lastPage.mailbox.next_offset;
      }
      const loadedPages = allPages.length;
      const lastPageRows = lastPage.mailbox.groups.reduce(
        (total, group) => total + group.rows.length,
        0,
      );
      return lastPageRows >= MAILBOX_PAGE_SIZE ? loadedPages * MAILBOX_PAGE_SIZE : undefined;
    },
    select: (data) => mergeMailboxPages(data.pages),
    enabled: shell.isSuccess,
    staleTime: 10_000,
  });
}

function mergeMailboxPages(pages: MailboxResponse[]): MailboxResponse | undefined {
  const first = pages[0];
  if (!first) return undefined;

  const groups: MessageGroupView[] = [];
  const groupIndexes = new Map<string, number>();
  const seenRows = new Set<string>();

  for (const page of pages) {
    for (const group of page.mailbox.groups) {
      const rows = group.rows.filter((row) => {
        if (seenRows.has(row.id)) return false;
        seenRows.add(row.id);
        return true;
      });
      if (rows.length === 0) continue;

      const existingIndex = groupIndexes.get(group.id);
      if (existingIndex === undefined) {
        groupIndexes.set(group.id, groups.length);
        groups.push({ ...group, rows });
      } else {
        const existing = groups[existingIndex];
        if (!existing) continue;
        groups[existingIndex] = {
          ...existing,
          rows: [...existing.rows, ...rows],
        };
      }
    }
  }

  return {
    ...first,
    mailbox: {
      ...first.mailbox,
      groups,
    },
  };
}
