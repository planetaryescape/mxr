/* Sender history panel: recent emails, volume, storage and timeline. */

import { Paperclip } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { ProfileRow, ProfileStat } from "./ProfileParts";
import { extractSenderProfile } from "./railPayloads";
import {
  formatBytes,
  formatCadence,
  formatDate,
  formatNumber,
  formatShortDate,
  interactionDirection,
} from "./railFormat";
import { RelationshipPanel } from "./RelationshipPanel";

export function SenderProfilePanel({ payload }: { payload: unknown }) {
  const profile = extractSenderProfile(payload);
  if (!profile) {
    return (
      <div className="rounded-md border border-border bg-muted/40 px-3 py-4 text-sm text-foreground">
        No sender history yet.
      </div>
    );
  }

  const totalEmails = profile.total_inbound + profile.total_outbound;
  const replyRate =
    profile.total_inbound > 0
      ? Math.round((profile.replied_count / profile.total_inbound) * 100)
      : 0;
  const inboundBytes = profile.inbound_storage_bytes ?? 0;
  const outboundBytes = profile.outbound_storage_bytes ?? 0;
  const totalBytes = inboundBytes + outboundBytes;
  const storageDelta = inboundBytes - outboundBytes;
  const emailDelta = profile.total_inbound - profile.total_outbound;
  const inboundShare =
    totalEmails > 0 ? Math.round((profile.total_inbound / totalEmails) * 100) : 0;
  const attachmentBytes = profile.attachment_bytes ?? 0;
  const attachmentShare = totalBytes > 0 ? Math.round((attachmentBytes / totalBytes) * 100) : 0;
  const avgInboundBytes = profile.total_inbound > 0 ? inboundBytes / profile.total_inbound : 0;
  const avgOutboundBytes = profile.total_outbound > 0 ? outboundBytes / profile.total_outbound : 0;
  const currentThreadId = currentThreadIdFromPath();
  const mailboxBase = mailboxBaseFromPath();
  const otherMessages = (profile.recent_messages ?? []).filter(
    (message) => message.thread_id !== currentThreadId,
  );

  return (
    <div className="space-y-4 text-foreground">
      <div className="space-y-2">
        <div>
          <h3 className="break-words text-sm font-semibold">
            {profile.display_name || profile.email}
          </h3>
          <div className="break-all font-mono text-2xs text-muted-foreground">{profile.email}</div>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {profile.is_list_sender ? <Badge variant="secondary">List sender</Badge> : null}
          {profile.open_thread_count > 0 ? (
            <Badge variant="outline">{profile.open_thread_count} open</Badge>
          ) : null}
        </div>
      </div>

      {otherMessages.length > 0 ? (
        <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
          <h4 className="text-xs font-medium">Other emails from sender</h4>
          <div className="space-y-1.5">
            {otherMessages.slice(0, 8).map((message) => (
              <a
                key={message.message_id}
                href={`${mailboxBase}/${message.thread_id}`}
                className="block rounded border border-border/70 bg-background/50 px-2.5 py-2 text-xs outline-none hover:border-accent hover:bg-accent/10 focus-visible:ring-2 focus-visible:ring-ring"
              >
                <div className="flex items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">
                      {message.subject.trim() || "(no subject)"}
                    </div>
                    {message.snippet ? (
                      <div className="mt-0.5 line-clamp-2 text-muted-foreground">
                        {message.snippet}
                      </div>
                    ) : null}
                  </div>
                  <div className="flex shrink-0 items-center gap-1 text-2xs text-muted-foreground">
                    {message.has_attachments ? <Paperclip className="size-3" /> : null}
                    <span>{formatShortDate(message.date)}</span>
                  </div>
                </div>
              </a>
            ))}
          </div>
        </div>
      ) : null}

      <div className="grid grid-cols-2 gap-2">
        <ProfileStat label="Emails" value={formatNumber(totalEmails)} />
        <ProfileStat label="Inbound" value={formatNumber(profile.total_inbound)} />
        <ProfileStat label="Outbound" value={formatNumber(profile.total_outbound)} />
        <ProfileStat label="Replies" value={formatNumber(profile.replied_count)} />
        <ProfileStat label="Reply rate" value={`${replyRate}%`} />
        <ProfileStat label="Inbound share" value={`${inboundShare}%`} />
        <ProfileStat label="Open threads" value={formatNumber(profile.open_thread_count)} />
        <ProfileStat label="Cadence" value={formatCadence(profile.cadence_days_p50)} />
      </div>

      <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
        <h4 className="text-xs font-medium">Interaction</h4>
        <ProfileRow label="Direction" value={interactionDirection(emailDelta)} />
        <ProfileRow
          label="Ratio"
          value={`${formatNumber(profile.total_inbound)} in / ${formatNumber(profile.total_outbound)} out`}
        />
        <ProfileRow label="List sender" value={profile.is_list_sender ? "Yes" : "No"} />
      </div>

      {profile.relationship ? (
        <RelationshipPanel payload={payload} relationship={profile.relationship} />
      ) : null}

      <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
        <h4 className="text-xs font-medium">Storage</h4>
        <ProfileRow label="From sender" value={formatBytes(inboundBytes)} />
        <ProfileRow label="To sender" value={formatBytes(outboundBytes)} />
        <ProfileRow label="Avg inbound" value={formatBytes(avgInboundBytes)} />
        <ProfileRow label="Avg outbound" value={formatBytes(avgOutboundBytes)} />
        <ProfileRow
          label="Asymmetry"
          value={
            storageDelta === 0
              ? "Balanced"
              : `${storageDelta > 0 ? "They send +" : "You send +"}${formatBytes(Math.abs(storageDelta))}`
          }
        />
        <ProfileRow
          label="Attachments"
          value={`${formatNumber(profile.attachment_count ?? 0)} · ${formatBytes(profile.attachment_bytes ?? 0)}`}
        />
        <ProfileRow label="Attach share" value={`${attachmentShare}% of stored bytes`} />
      </div>

      <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
        <h4 className="text-xs font-medium">Timeline</h4>
        <ProfileRow label="First seen" value={formatDate(profile.first_seen_at)} />
        <ProfileRow label="Last seen" value={formatDate(profile.last_seen_at)} />
        <ProfileRow label="Last inbound" value={formatDate(profile.last_inbound_at)} />
        <ProfileRow label="Last outbound" value={formatDate(profile.last_outbound_at)} />
      </div>

      <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
        <h4 className="text-xs font-medium">Balance</h4>
        <ProfileRow
          label="Email asymmetry"
          value={
            emailDelta === 0
              ? "Balanced"
              : `${emailDelta > 0 ? "They send +" : "You send +"}${formatNumber(Math.abs(emailDelta))}`
          }
        />
        {profile.list_id ? <ProfileRow label="List ID" value={profile.list_id} /> : null}
      </div>
    </div>
  );
}

function currentThreadIdFromPath(): string | null {
  const parts = window.location.pathname.split("/").filter(Boolean);
  if (parts.length >= 3 && parts[0] === "m") return parts[2] ?? null;
  return null;
}

function mailboxBaseFromPath(): string {
  const parts = window.location.pathname.split("/").filter(Boolean);
  if (parts.length >= 2 && parts[0] === "m") return `/${parts.slice(0, 2).join("/")}`;
  return "/m/inbox";
}
