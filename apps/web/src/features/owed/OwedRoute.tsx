import { useQuery } from "@tanstack/react-query";
import { Hourglass } from "lucide-react";

import { fetchOwedReplies, type OwedPeople, type OwedReplyRow } from "./api";
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
 * The same count Now, Messages and the rail use: the people you write
 * with, then where the others who wrote to you are.
 */
export function owedMeta(conversations: number, people?: OwedPeople | null): string | null {
  if (conversations === 0) return null;
  if (!people) return plural(conversations, "conversation");
  const who = plural(people.write_with, "person", "people");
  const others = people.others > 0 ? `, ${plural(people.others, "other")} in Messages` : "";
  return conversations === people.write_with
    ? `${who}${others}`
    : `${who} (${plural(conversations, "conversation")})${others}`;
}

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
      purpose="People waiting on you, most overdue first."
      meta={owedMeta(rows.length, owed.data?.people)}
      groups={groups}
      scopeKey={`owed|${account ?? "all"}`}
      rowGists
      status={owed}
      empty={
        <Centered
          icon={<Hourglass className="size-6" />}
          title="You don't owe anyone you write with a reply"
          body={
            owed.data?.people?.others
              ? `${plural(owed.data.people.others, "other")} who wrote to you ${owed.data.people.others === 1 ? "is" : "are"} in Messages.`
              : "Threads where someone is waiting on you show up here, ranked by how overdue they are."
          }
        />
      }
    />
  );
}
