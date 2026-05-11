import { X } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { AttachmentActions } from "@/features/thread/AttachmentActions";
import type { AttachmentView } from "@/features/mailbox/types";
import { useModals } from "@/state/modalStore";

export function RightRail() {
  const rail = useModals((s) => s.rightRail);
  const close = useModals((s) => s.closeRightRail);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && rail) close();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [rail, close]);

  if (!rail) return null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 items-center justify-between border-b border-border px-3">
        <div className="text-xs font-medium capitalize">{rail.kind.replace(/-/g, " ")}</div>
        <Button variant="ghost" size="icon" onClick={close} aria-label="Close panel">
          <X className="size-3" />
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="p-3 text-xs text-muted-foreground">
          <RailContent kind={rail.kind} payload={rail.payload} />
        </div>
      </ScrollArea>
    </div>
  );
}

function RailContent({ kind, payload }: { kind: string; payload: unknown }) {
  if (kind === "thread-context" && isThreadContext(payload)) {
    return (
      <div className="space-y-3">
        <h3 className="text-sm font-medium text-foreground">{payload.title ?? "Thread context"}</h3>
        <ul className="space-y-2">
          {(payload.items ?? []).map((item) => (
            <li key={item} className="rounded-md border border-border bg-muted/40 px-3 py-2">
              {item}
            </li>
          ))}
        </ul>
      </div>
    );
  }
  if (kind === "attachments" && Array.isArray(payload)) {
    return (
      <div className="space-y-2">
        {payload.map((item, index) => {
          const attachment = item as AttachmentView;
          return <AttachmentActions key={attachment.id ?? index} attachment={attachment} />;
        })}
      </div>
    );
  }
  if (kind === "sender-profile") {
    return <SenderProfilePanel payload={payload} />;
  }
  return <pre className="font-mono text-2xs">{JSON.stringify(payload ?? null, null, 2)}</pre>;
}

function isThreadContext(value: unknown): value is { title?: string; items?: string[] } {
  return typeof value === "object" && value !== null && "items" in value;
}

interface SenderProfile {
  account_id: string;
  email: string;
  display_name?: string | null;
  first_seen_at: string;
  last_seen_at: string;
  last_inbound_at?: string | null;
  last_outbound_at?: string | null;
  total_inbound: number;
  total_outbound: number;
  replied_count: number;
  cadence_days_p50?: number | null;
  is_list_sender: boolean;
  list_id?: string | null;
  open_thread_count: number;
  inbound_storage_bytes?: number;
  outbound_storage_bytes?: number;
  attachment_count?: number;
  attachment_bytes?: number;
}

function SenderProfilePanel({ payload }: { payload: unknown }) {
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

function ProfileStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-muted/30 px-3 py-2">
      <div className="font-mono text-base font-semibold">{value}</div>
      <div className="mt-0.5 text-2xs uppercase tracking-wide text-muted-foreground">{label}</div>
    </div>
  );
}

function ProfileRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[92px_1fr] gap-2 text-xs">
      <div className="text-muted-foreground">{label}</div>
      <div className="min-w-0 break-words text-right font-medium">{value}</div>
    </div>
  );
}

function extractSenderProfile(value: unknown): SenderProfile | null {
  if (!isRecord(value)) return null;
  const candidate = isRecord(value.profile) ? value.profile : value;
  if (typeof candidate.email !== "string") return null;
  return candidate as unknown as SenderProfile;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value);
}

function formatBytes(value: number): string {
  if (value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${new Intl.NumberFormat(undefined, {
    maximumFractionDigits: size >= 10 || unit === 0 ? 0 : 1,
  }).format(size)} ${units[unit]}`;
}

function formatCadence(value?: number | null): string {
  if (value == null) return "Unknown";
  if (value < 1) return "<1d";
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value)}d`;
}

function interactionDirection(delta: number): string {
  if (delta === 0) return "Balanced";
  return delta > 0 ? "Mostly inbound" : "Mostly outbound";
}

function formatDate(value?: string | null): string {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}
