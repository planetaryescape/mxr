/* Open commitments with the conversation's first sender, and resolving them. */

import { useMutation, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { toast } from "sonner";

import { listCommitments, resolveCommitment } from "@/features/mailbox/api";
import type { MessageRowView, ThreadResponse } from "@/features/mailbox/types";
import { parseAddress } from "@/lib/format";
import { extractThreadCommitments } from "./ThreadInsights";

export function useThreadCommitments(data: ThreadResponse, messages: MessageRowView[]) {
  const primaryEmail = parseAddress(messages[0]?.sender_detail ?? messages[0]?.sender).email;
  const commitments = useQuery({
    queryKey: ["commitments", data.thread.account_id, primaryEmail],
    queryFn: () =>
      listCommitments({
        accountId: data.thread.account_id,
        email: primaryEmail ?? undefined,
        status: "open",
      }),
    enabled: Boolean(primaryEmail),
    staleTime: 30_000,
  });
  const resolve = useMutation({
    mutationFn: resolveCommitment,
    onSuccess: () => {
      toast.success("Commitment resolved");
      void commitments.refetch();
    },
    onError: (error) => toast.error("Couldn't resolve", { description: error.message }),
  });
  const openCommitments = useMemo(
    () => extractThreadCommitments(commitments.data),
    [commitments.data],
  );

  return { openCommitments, resolve };
}
