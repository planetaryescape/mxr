/*
 * A list's selection kept in the URL, so a copied link reopens the same
 * row. The route declares the key in its validateSearch; this reads it and
 * rewrites it in place (`replace`), so moving the cursor adds no history.
 */

import { useNavigate, useRouter, useSearch } from "@tanstack/react-router";
import { useCallback } from "react";

import { optionalString } from "@/lib/searchParams";

type Search = Record<string, unknown>;

/** Patch the current URL's search params in place; `undefined` drops a key. */
export function useReplaceSearch(): (patch: Search) => void {
  const navigate = useNavigate();
  const router = useRouter();
  return useCallback(
    (patch: Search) => {
      const current = router.latestLocation.search as Search;
      if (Object.entries(patch).every(([key, value]) => current[key] === value)) return;
      void navigate({
        to: ".",
        search: (prev: Search) => ({ ...prev, ...patch }),
        replace: true,
      } as never);
    },
    [navigate, router],
  );
}

/** One string search param as state: `null` when absent. */
export function useSearchParam(key: string): [string | null, (next: string | null) => void] {
  const value = useSearch({
    strict: false,
    select: (search: Search) => optionalString(search[key]) ?? null,
  } as never) as string | null;
  const replace = useReplaceSearch();
  const set = useCallback(
    (next: string | null) => replace({ [key]: next ?? undefined }),
    [key, replace],
  );
  return [value, set];
}

/** The keys of `search` a list keeps across opening and closing an email. */
export function pickSearch(search: Search, keys: readonly string[]): Search {
  return Object.fromEntries(keys.flatMap((key) => (key in search ? [[key, search[key]]] : [])));
}
