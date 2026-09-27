import { useMutation, useQuery } from "@tanstack/react-query";
import { AlarmClockOff, Clock } from "lucide-react";
import { toast } from "sonner";

import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { fetchSnoozed, unsnoozeMessage, type SnoozedEntry } from "@/features/mailbox/api";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import { Centered } from "@/features/mailbox/MailViewParts";
import type { MessageRowView } from "@/features/mailbox/types";
import { formatLongDate, formatRelative, plural } from "@/lib/format";

/** One row per conversation: waking it wakes every snoozed message in it. */
function toRows(entries: SnoozedEntry[]): MessageRowView[] {
  const byThread = new Map<string, SnoozedEntry[]>();
  for (const entry of entries) {
    const key = entry.thread_id ?? entry.message_id;
    byThread.set(key, [...(byThread.get(key) ?? []), entry]);
  }
  return [...byThread.entries()]
    .map(([threadId, group]) => {
      const sorted = group.toSorted((a, b) => a.wake_at.localeCompare(b.wake_at));
      const first = sorted[0]!;
      return {
        id: first.message_id,
        kind: sorted.length > 1 ? "thread" : "message",
        message_ids: sorted.map((entry) => entry.message_id),
        message_count: sorted.length,
        thread_id: threadId,
        provider_id: "",
        sender: first.sender ?? "",
        subject: first.subject ?? "(no subject)",
        snippet: `Back ${formatRelative(first.wake_at)} (${formatLongDate(first.wake_at)})`,
        date: first.wake_at,
        date_label: "",
        date_full: first.wake_at,
        date_relative: "",
        unread: sorted.some((entry) => entry.unread),
        starred: false,
        has_attachments: sorted.some((entry) => entry.has_attachments),
      };
    })
    .toSorted((a, b) => a.date.localeCompare(b.date));
}

const LENS = { kind: "other" } as const;

/** Snoozed mail, next to wake first, with a manual wake (the TUI has no unsnooze). */
export function SnoozedRoute() {
  const snoozed = useQuery({ queryKey: ["snoozed"], queryFn: fetchSnoozed });
  const rows = toRows(snoozed.data?.snoozed ?? []);
  const groups = useProjectedGroups(
    rows.length > 0 ? [{ id: "snoozed", label: "Waking soonest first", rows }] : [],
    LENS,
  );
  const wake = useMutation({
    mutationFn: (ids: string[]) => Promise.all(ids.map((id) => unsnoozeMessage(id))),
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
      meta={rows.length > 0 ? plural(rows.length, "conversation") : null}
      groups={groups}
      scopeKey="snoozed"
      status={snoozed}
      rowAction={{
        label: "Wake now",
        icon: AlarmClockOff,
        describe: (row) => `Wake ${row.subject} now`,
        run: (row) => wake.mutate(row.message_ids?.length ? row.message_ids : [row.id]),
      }}
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
