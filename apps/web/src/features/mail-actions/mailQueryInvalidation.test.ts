import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import { MESSAGES_KEY } from "@/features/messages/api";

import { invalidateMailQueries } from "./mailQueryInvalidation";

describe("invalidateMailQueries", () => {
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
