import { useInfiniteQuery, useQueries } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";

import { useUiPrefs } from "@/state/uiPrefsStore";

import {
  fetchPlace,
  type Place,
  type PlaceBundle,
  type PlaceMessage,
  type PlaceResponse,
} from "./api";
import { bundleKey } from "./placeCopy";
import { mergeBundles, messagesLeft, nextBundleOffset } from "./placePaging";

const NO_PAGES: Record<string, SenderPages> = {};

/** Further message pages asked for one sender, keyed by `bundleKey`. */
interface SenderPages {
  accountId: string;
  senderEmail: string;
  pages: number;
}

/**
 * Module-level so its reference is stable: TanStack Query then reruns it
 * (and hands back a new object) only when a sender page changes.
 */
function combineSenderPages(results: readonly { data?: PlaceResponse; isFetching: boolean }[]): {
  data: (PlaceResponse | undefined)[];
  fetching: boolean[];
} {
  return {
    data: results.map((result) => result.data),
    fetching: results.map((result) => result.isFetching),
  };
}

/**
 * A place for the app's account scope, paged: bundles a page of senders at
 * a time, and each sender's messages `messagesPerBundle` at a time. Every
 * page lives under the `place` query family, so a mail mutation refreshes
 * them all and optimistic pin and move patches reach every copy.
 */
export function usePlace(place: Place, messagesPerBundle: number) {
  const account = useUiPrefs((s) => s.accountScope);
  const scope = account ?? "all";
  const bundles = useInfiniteQuery({
    queryKey: ["place", place, scope, "bundles", messagesPerBundle],
    queryFn: ({ pageParam }) =>
      fetchPlace(place, { account, offset: pageParam, messagesPerBundle }),
    initialPageParam: 0,
    getNextPageParam: (_last, pages) => nextBundleOffset(pages),
    staleTime: 15_000,
  });

  // Pages asked for belong to one account scope; a new scope starts over.
  const [asked, setAsked] = useState<{ scope: string; pages: Record<string, SenderPages> }>({
    scope,
    pages: {},
  });
  const senderPages = asked.scope === scope ? asked.pages : NO_PAGES;
  const requests = useMemo(
    () =>
      Object.entries(senderPages).flatMap(([key, sender]) =>
        Array.from({ length: sender.pages }, (_, index) => ({ key, sender, page: index + 1 })),
      ),
    [senderPages],
  );
  const extraPages = useQueries({
    queries: requests.map(({ key, sender, page }) => ({
      queryKey: ["place", place, scope, "sender", key, messagesPerBundle, page],
      queryFn: () =>
        fetchPlace(place, {
          account: sender.accountId,
          sender: sender.senderEmail,
          messagesPerBundle,
          messageOffset: page * messagesPerBundle,
        }),
      staleTime: 15_000,
    })),
    combine: combineSenderPages,
  });

  const pages = bundles.data?.pages;
  const merged = useMemo(() => {
    const more = new Map<string, PlaceMessage[]>();
    requests.forEach(({ key }, index) => {
      const messages = extraPages.data[index]?.bundles[0]?.messages ?? [];
      more.set(key, [...(more.get(key) ?? []), ...messages]);
    });
    return mergeBundles(pages ?? [], more);
  }, [extraPages.data, pages, requests]);

  const loadingKeys = useMemo(
    () => new Set(requests.filter((_, index) => extraPages.fetching[index]).map(({ key }) => key)),
    [extraPages.fetching, requests],
  );

  /** Ask for the next page of these senders' messages. */
  const moreFrom = useCallback(
    (targets: readonly PlaceBundle[]) =>
      setAsked((state) => {
        const previous = state.scope === scope ? state.pages : NO_PAGES;
        const next = { ...previous };
        for (const bundle of targets) {
          if (messagesLeft(bundle) === 0) continue;
          const key = bundleKey(bundle);
          next[key] = {
            accountId: bundle.account_id,
            senderEmail: bundle.sender_email,
            pages: (previous[key]?.pages ?? 0) + 1,
          };
        }
        return { scope, pages: next };
      }),
    [scope],
  );

  const first = pages?.[0];
  return {
    status: bundles,
    bundles: merged,
    accountId: first?.account_id ?? account,
    totalBundles: first?.total_bundles ?? 0,
    totalMessages: first?.total_messages ?? 0,
    hasMoreSenders: bundles.hasNextPage,
    loadingMoreSenders: bundles.isFetchingNextPage,
    loadMoreSenders: () => void bundles.fetchNextPage(),
    moreFrom,
    loadingFrom: loadingKeys,
  };
}
