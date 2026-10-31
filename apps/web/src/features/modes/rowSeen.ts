import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";

export interface SeenRow {
  id: string;
  account_id: string;
  unread?: boolean | null;
  seen_revision?: string | null;
}

/** Called on opening/peeking only, never on selection or list fetch. A late
 * response acknowledges only the revision shown, not newly arrived content. */
export function useMarkRowSeen(mode: "todo" | "updates") {
  const client = useQueryClient();
  const { mutate } = useMutation({
    mutationFn: (row: SeenRow) =>
      apiFetch("/api/v1/mail/rows/seen", {
        method: "POST",
        body: { account_id: row.account_id, mode, row_id: row.id, revision: row.seen_revision },
      }),
    onSuccess: () =>
      client.invalidateQueries({ queryKey: [mode === "todo" ? "todos" : "updates"] }),
    onError: () => toast.error("Couldn't save that this row was seen. Open it again to retry."),
  });
  return useCallback(
    (row: SeenRow) => {
      // Older daemons have no row revision and must not receive a new request.
      if (row.unread && row.seen_revision) mutate(row);
    },
    [mutate],
  );
}
