import { useMutation, useQuery } from "@tanstack/react-query";
import { AlarmClockOff, Clock } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { fetchSnoozed, unsnoozeMessage, type SnoozedEntry } from "@/features/mailbox/api";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import { Centered } from "@/features/mailbox/MailViewParts";
import type { MessageRowView } from "@/features/mailbox/types";
import { formatLongDate, formatRelative, plural } from "@/lib/format";

function toRow(entry: SnoozedEntry): MessageRowView {
  return {
    id: entry.message_id,
    kind: "message",
    thread_id: entry.thread_id ?? entry.message_id,
    provider_id: "",
    sender: entry.sender ?? "",
    subject: entry.subject ?? "(no subject)",
    snippet: `Back ${formatRelative(entry.wake_at)} (${formatLongDate(entry.wake_at)})`,
    date: entry.wake_at,
    date_label: "",
    date_full: entry.wake_at,
    date_relative: "",
    unread: entry.unread ?? false,
    starred: false,
    has_attachments: entry.has_attachments ?? false,
  };
}

const LENS = { kind: "other" } as const;

/** Snoozed mail, next to wake first, with a manual wake (the TUI has no unsnooze). */
export function SnoozedRoute() {
  const snoozed = useQuery({ queryKey: ["snoozed"], queryFn: fetchSnoozed });
  const rows = (snoozed.data?.snoozed ?? [])
    .toSorted((a, b) => a.wake_at.localeCompare(b.wake_at))
    .map(toRow);
  const groups = useProjectedGroups(
    rows.length > 0 ? [{ id: "snoozed", label: "Waking soonest first", rows }] : [],
    LENS,
  );
  const wake = useMutation({
    mutationFn: unsnoozeMessage,
    onSuccess: () => {
      toast.success("Back in your inbox");
      void invalidateMailQueries();
    },
    onError: (error) => toast.error("Couldn't wake it", { description: error.message }),
  });
  return (
    <ListWithReader
      basePath="/snoozed"
      title="Snoozed"
      meta={rows.length > 0 ? plural(rows.length, "message") : null}
      groups={groups}
      scopeKey="snoozed"
      status={snoozed}
      rowAction={(row) => (
        <Button
          variant="ghost"
          size="xs"
          disabled={wake.isPending}
          onClick={() => wake.mutate(row.id)}
          aria-label={`Wake ${row.subject} now`}
        >
          <AlarmClockOff className="size-3" /> Wake now
        </Button>
      )}
      empty={
        <Centered
          icon={<Clock className="size-6" />}
          title="Nothing snoozed"
          body="Press Z on a conversation to send it away until later."
        />
      }
    />
  );
}
