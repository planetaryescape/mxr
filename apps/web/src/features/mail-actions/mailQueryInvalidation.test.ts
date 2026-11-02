import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import { MESSAGES_KEY } from "@/features/messages/api";
import { TODO_KEY } from "@/features/todo/api";

import { invalidateMailQueries } from "./mailQueryInvalidation";

describe("invalidateMailQueries", () => {
  it("refreshes an open empty To do runway when sync adds a task", async () => {
    const qc = new QueryClient();
    const key = [...TODO_KEY, "runway", "all"];
    let serverTitles: string[] = [];
    qc.setQueryData(key, []);
    qc.setQueryData([...TODO_KEY, "catchup", "all"], []);
    qc.setQueryData([...TODO_KEY, "expired", "all"], []);
    qc.setQueryData(["accounts"], []);
    const observer = new QueryObserver(qc, {
      queryKey: key,
      queryFn: async () => serverTitles,
      staleTime: Infinity,
    });
    const unsubscribe = observer.subscribe(() => {});
    try {
      serverTitles = ["Sign the newly arrived contract"];
      await invalidateMailQueries(qc);
      expect(observer.getCurrentResult().data).toEqual(["Sign the newly arrived contract"]);
      expect(qc.getQueryState([...TODO_KEY, "catchup", "all"])?.isInvalidated).toBe(true);
      expect(qc.getQueryState([...TODO_KEY, "expired", "all"])?.isInvalidated).toBe(true);
      expect(qc.getQueryState(["accounts"])?.isInvalidated).toBe(false);
    } finally {
      unsubscribe();
      qc.clear();
    }
  });

  it("refreshes Messages, so Not now from Messages takes the person off the list", async () => {
    const qc = new QueryClient();
    qc.setQueryData([...MESSAGES_KEY, "list", "all", "attention"], { rows: [] });
    qc.setQueryData([...MESSAGES_KEY, "person", "all", "jon", null], { row: {} });
    qc.setQueryData(["accounts"], []);

    await invalidateMailQueries(qc);

    expect(qc.getQueryState([...MESSAGES_KEY, "list", "all", "attention"])?.isInvalidated).toBe(
      true,
    );
    expect(qc.getQueryState([...MESSAGES_KEY, "person", "all", "jon", null])?.isInvalidated).toBe(
      true,
    );
    // Not a view of mail: left alone.
    expect(qc.getQueryState(["accounts"])?.isInvalidated).toBe(false);
  });
});
