import { useQuery } from "@tanstack/react-query";
import { Hourglass } from "lucide-react";

import { fetchOwedReplies, type OwedReplyRow } from "./api";
import { waitingLine } from "./waitingLine";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import { Centered } from "@/features/mailbox/MailViewParts";
import type { MessageRowView } from "@/features/mailbox/types";
import { plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

function toRow(row: OwedReplyRow): MessageRowView {
  return {
    id: row.latest_inbound_msg_id,
    kind: "message",
    thread_id: row.thread_id,
    provider_id: "",
    sender: row.from_name?.trim() || row.from_email,
    sender_detail: row.from_email,
    subject: row.subject,
    snippet: waitingLine(row),
    date: row.latest_inbound_at,
    date_label: "",
    date_full: row.latest_inbound_at,
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
  };
}

const LENS = { kind: "other" } as const;

/**
 * The desk's You owe lane in full: mail from people you're in conversation
 * with that you haven't answered, most overdue first by your usual pace
 * with that person (TUI Owed lens).
 */
export function OwedRoute() {
  const account = useUiPrefs((s) => s.accountScope);
  const owed = useQuery({ queryKey: ["owed", account], queryFn: () => fetchOwedReplies(account) });
  const rows = (owed.data?.rows ?? []).map(toRow);
  const groups = useProjectedGroups(
    rows.length > 0 ? [{ id: "owed", label: "Most overdue first", rows }] : [],
    LENS,
  );
  return (
    <ListWithReader
      basePath="/owed"
      title="Owed replies"
      meta={rows.length > 0 ? plural(rows.length, "conversation") : null}
      groups={groups}
      scopeKey={`owed|${account ?? "all"}`}
      rowGists
      status={owed}
      empty={
        <Centered
          icon={<Hourglass className="size-6" />}
          title="You don't owe anyone a reply"
          body="Threads where someone is waiting on you show up here, ranked by how overdue they are."
        />
      }
    />
  );
}
