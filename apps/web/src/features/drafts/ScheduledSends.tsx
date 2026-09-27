/*
 * "Scheduled": drafts waiting to send at a set time, soonest first. Only
 * shown when something is scheduled. Cancelling keeps the draft, so it is
 * reversible by scheduling again and needs no confirmation.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Clock } from "lucide-react";
import { toast } from "sonner";

import { cancelScheduledSend, fetchScheduledSends, type ScheduledSend } from "./api";
import { PageSection } from "@/components/Page";
import { PageError, RuledList, RuledRow } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { formatLongDate, formatRelative } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

const OUTCOME_TEXT: Record<NonNullable<ScheduledSend["last_attempt_outcome"]>, string> = {
  sent: "Last attempt reported sent",
  blocked: "Last attempt was blocked by a safety check",
  failed: "Last attempt failed",
  interrupted: "Last attempt was interrupted",
};

function recipients(send: ScheduledSend): string {
  const all = [...send.to, ...send.cc, ...send.bcc];
  if (all.length === 0) return "no recipients";
  return all.map((address) => address.name || address.email).join(", ");
}

/** An older bridge without the endpoint answers 404 or 405: nothing to show. */
function isMissingEndpoint(error: Error): boolean {
  return "status" in error && (error.status === 404 || error.status === 405);
}

export function ScheduledSends() {
  const qc = useQueryClient();
  const account = useUiPrefs((state) => state.accountScope);
  const sends = useQuery({
    queryKey: ["scheduled-sends", account],
    queryFn: () => fetchScheduledSends(account),
    retry: (count, error) => !isMissingEndpoint(error) && count < 2,
  });
  const cancel = useMutation({
    mutationFn: (send: ScheduledSend) => cancelScheduledSend(send.draft_id),
    onSuccess: (_result, send) => {
      toast.success("Send cancelled", {
        description: `"${send.subject.trim() || "(no subject)"}" is back in your drafts.`,
      });
      void qc.invalidateQueries({ queryKey: ["scheduled-sends"] });
      void qc.invalidateQueries({ queryKey: ["drafts"] });
    },
    onError: (error) => toast.error("Could not cancel the send", { description: error.message }),
  });

  if (sends.isError && !isMissingEndpoint(sends.error))
    return (
      <PageSection title="Scheduled">
        <PageError
          title="Scheduled sends unavailable"
          error={sends.error}
          onRetry={() => void sends.refetch()}
        />
      </PageSection>
    );
  const rows = sends.data ?? [];
  if (rows.length === 0) return null;

  return (
    <PageSection title="Scheduled" description="These send on their own at the time shown.">
      <RuledList label="Scheduled sends">
        {rows.map((send) => (
          <RuledRow
            key={send.draft_id}
            title={send.subject.trim() || "(no subject)"}
            meta={
              <>
                <span className="inline-flex items-center gap-1">
                  <Clock className="size-3" aria-hidden="true" />
                  {formatLongDate(send.send_at)} ({formatRelative(send.send_at)})
                </span>
                {" · "}
                {recipients(send)}
                {send.last_attempt_outcome ? (
                  <span className="mt-0.5 flex items-center gap-1 text-warning">
                    <AlertTriangle className="size-3" aria-hidden="true" />
                    {OUTCOME_TEXT[send.last_attempt_outcome]}
                    {send.last_attempt_at ? ` ${formatRelative(send.last_attempt_at)}` : ""}
                  </span>
                ) : null}
              </>
            }
            actions={
              <Button
                variant="outline"
                size="xs"
                disabled={cancel.isPending}
                onClick={() => cancel.mutate(send)}
              >
                Cancel send
              </Button>
            }
          />
        ))}
      </RuledList>
    </PageSection>
  );
}
