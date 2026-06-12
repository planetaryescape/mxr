import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Paperclip, X } from "lucide-react";
import { useEffect } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { resolveCommitment as resolveCommitmentApi } from "@/features/mailbox/api";
import { ExpertFinderPanel } from "@/features/mailbox/ExpertFinderPanel";
import { LabelPicker } from "@/features/mailbox/LabelPicker";
import { MovePicker } from "@/features/mailbox/MovePicker";
import { RoutePicker } from "@/features/mailbox/RoutePicker";
import { AttachmentActions } from "@/features/thread/AttachmentActions";
import { DraftAssistPanel } from "@/features/thread/DraftAssistPanel";
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
  if (kind === "label-picker" && isLabelPickerPayload(payload)) {
    return (
      <LabelPicker
        mode={payload.mode}
        messageIds={payload.messageIds}
        appliedLabels={payload.appliedLabels}
        onClose={() => useModals.getState().closeRightRail()}
      />
    );
  }
  if (kind === "move-picker" && isMovePickerPayload(payload)) {
    return (
      <MovePicker
        messageIds={payload.messageIds}
        onClose={() => useModals.getState().closeRightRail()}
      />
    );
  }
  if (kind === "route-picker" && isRoutePickerPayload(payload)) {
    return (
      <RoutePicker
        messageIds={payload.messageIds}
        fromQueueLabel={payload.fromQueueLabel}
        archive={payload.archive}
        onClose={() => useModals.getState().closeRightRail()}
      />
    );
  }
  if (kind === "draft-assist" && isDraftAssistPayload(payload)) {
    return <DraftAssistPanel threadId={payload.threadId} />;
  }
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
    const attachments = payload.filter(isAttachmentView);
    return (
      <div className="space-y-2">
        {attachments.map((attachment, index) => (
          <AttachmentActions key={attachment.id ?? index} attachment={attachment} />
        ))}
      </div>
    );
  }
  if (kind === "sender-profile") {
    return <SenderProfilePanel payload={payload} />;
  }
  if (kind === "commitments") {
    return <CommitmentsPanel payload={payload} />;
  }
  if (kind === "thread-briefing" || kind === "recipient-briefing") {
    return <BriefingPanel payload={payload} />;
  }
  if (kind === "expert-finder") {
    return <ExpertFinderPanel />;
  }
  return <pre className="font-mono text-2xs">{JSON.stringify(payload ?? null, null, 2)}</pre>;
}

function isThreadContext(value: unknown): value is { title?: string; items?: string[] } {
  return typeof value === "object" && value !== null && "items" in value;
}

interface LabelPickerPayload {
  mode: "label-add" | "label-remove";
  messageIds: string[];
  appliedLabels?: string[];
}

function isLabelPickerPayload(value: unknown): value is LabelPickerPayload {
  if (!isRecord(value)) return false;
  if (value.mode !== "label-add" && value.mode !== "label-remove") return false;
  return Array.isArray(value.messageIds);
}

interface MovePickerPayload {
  messageIds: string[];
}

function isMovePickerPayload(value: unknown): value is MovePickerPayload {
  return isRecord(value) && Array.isArray(value.messageIds);
}

interface RoutePickerPayload {
  messageIds: string[];
  fromQueueLabel: string;
  archive?: boolean;
}

function isRoutePickerPayload(value: unknown): value is RoutePickerPayload {
  return (
    isRecord(value) && Array.isArray(value.messageIds) && typeof value.fromQueueLabel === "string"
  );
}

interface DraftAssistPayload {
  threadId: string;
}

function isDraftAssistPayload(value: unknown): value is DraftAssistPayload {
  return isRecord(value) && typeof value.threadId === "string";
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
  recent_messages?: SenderEmailReference[];
  relationship?: RelationshipProfile | null;
}

interface RelationshipProfile {
  style?: ContactStyle | null;
  summary?: RelationshipSummary | null;
  open_commitments?: Commitment[];
  drift?: RelationshipDrift | null;
}

interface ContactStyle {
  formality_score: number;
  formality_score_theirs: number;
  avg_sentence_len: number;
  avg_sentence_len_theirs: number;
  msg_count_used: number;
  msg_count_used_theirs: number;
}

interface RelationshipSummary {
  text: string;
  known_topics?: string[];
}

interface Commitment {
  id: string;
  account_id?: string;
  email?: string;
  thread_id?: string;
  direction: string;
  status?: string;
  who_owes: string;
  what: string;
  by_when?: string | null;
}

interface RelationshipDrift {
  detected_at: string;
  reason: string;
}

interface SenderEmailReference {
  message_id: string;
  thread_id: string;
  subject: string;
  snippet: string;
  from_name?: string | null;
  from_email: string;
  date: string;
  direction: string;
  has_attachments: boolean;
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
                className="block rounded border border-border/70 bg-background/50 px-2.5 py-2 text-xs outline-none transition hover:border-accent hover:bg-accent/10 focus-visible:ring-2 focus-visible:ring-ring"
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

function RelationshipPanel({
  payload,
  relationship,
}: {
  payload: unknown;
  relationship: RelationshipProfile;
}) {
  const commitments = relationship.open_commitments ?? [];
  const queryClient = useQueryClient();
  const openRail = useModals((state) => state.openRightRail);
  const resolveCommitment = useMutation({
    mutationFn: resolveCommitmentApi,
    onSuccess: (_result, commitmentId) => {
      openRail("sender-profile", removeCommitmentFromSenderPayload(payload, commitmentId));
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      toast.success("Commitment resolved");
    },
    onError: (error) => toast.error("Resolve failed", { description: error.message }),
  });
  return (
    <div className="space-y-3 rounded-md border border-border bg-muted/30 p-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-xs font-medium">Relationship</h4>
        {commitments.length > 0 ? <Badge variant="outline">{commitments.length} open</Badge> : null}
      </div>
      {relationship.drift ? (
        <div className="rounded-md border border-warning/40 bg-warning/10 px-2.5 py-2 text-2xs text-foreground">
          Voice drift: {relationship.drift.reason}
        </div>
      ) : null}
      {relationship.summary?.text ? (
        <p className="text-xs leading-relaxed text-muted-foreground">{relationship.summary.text}</p>
      ) : null}
      {relationship.summary?.known_topics?.length ? (
        <div className="flex flex-wrap gap-1">
          {relationship.summary.known_topics.slice(0, 12).map((topic) => (
            <Badge key={topic} variant="secondary" className="text-2xs">
              {topic}
            </Badge>
          ))}
        </div>
      ) : null}
      {relationship.style ? (
        <div className="grid gap-1.5">
          <ProfileRow label="Your style" value={styleSummary(relationship.style.formality_score)} />
          <ProfileRow
            label="Their style"
            value={styleSummary(relationship.style.formality_score_theirs)}
          />
          <ProfileRow
            label="Samples"
            value={`${relationship.style.msg_count_used} yours / ${relationship.style.msg_count_used_theirs} theirs`}
          />
        </div>
      ) : null}
      {commitments.length > 0 ? (
        <div className="space-y-1.5">
          {commitments.slice(0, 5).map((commitment) => (
            <div
              key={commitment.id}
              className="rounded border border-border/70 bg-background/50 px-2 py-1.5"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="font-medium">{commitment.what}</div>
                  <div className="mt-0.5 text-2xs text-muted-foreground">
                    {commitment.who_owes} · {commitment.direction}
                    {commitment.by_when ? ` · due ${formatShortDate(commitment.by_when)}` : ""}
                  </div>
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-6 shrink-0 px-2 text-2xs"
                  disabled={resolveCommitment.isPending}
                  onClick={() => resolveCommitment.mutate(commitment.id)}
                >
                  <CheckCircle2 className="size-3" />
                  Resolve
                </Button>
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function CommitmentsPanel({ payload }: { payload: unknown }) {
  const commitments = extractCommitments(payload);
  const queryClient = useQueryClient();
  const openRail = useModals((state) => state.openRightRail);
  const resolveCommitment = useMutation({
    mutationFn: resolveCommitmentApi,
    onSuccess: (_result, commitmentId) => {
      openRail("commitments", {
        commitments: commitments.filter((item) => item.id !== commitmentId),
      });
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      toast.success("Commitment resolved");
    },
    onError: (error) => toast.error("Resolve failed", { description: error.message }),
  });

  if (commitments.length === 0) {
    return (
      <div className="rounded-md border border-border bg-muted/40 px-3 py-4 text-sm text-foreground">
        No open commitments.
      </div>
    );
  }

  return (
    <div className="space-y-2 text-foreground">
      <h3 className="text-sm font-semibold">Open commitments</h3>
      {commitments.map((commitment) => (
        <div key={commitment.id} className="rounded-md border border-border bg-muted/30 p-3">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0 space-y-1">
              <div className="text-xs font-medium">{commitment.what}</div>
              <div className="text-2xs text-muted-foreground">
                {commitment.who_owes} · {commitment.direction}
                {commitment.email ? ` · ${commitment.email}` : ""}
                {commitment.by_when ? ` · due ${formatShortDate(commitment.by_when)}` : ""}
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 shrink-0 px-2 text-2xs"
              disabled={resolveCommitment.isPending}
              onClick={() => resolveCommitment.mutate(commitment.id)}
            >
              <CheckCircle2 className="size-3" />
              Resolve
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}

interface Briefing {
  thread_id: string;
  body_markdown: string;
  citations?: { message_id?: string; subject?: string; date?: string }[];
  generated_at: string;
  from_cache: boolean;
}

function BriefingPanel({ payload }: { payload: unknown }) {
  const briefing = extractBriefing(payload);
  if (!briefing) {
    return (
      <div className="rounded-md border border-border bg-muted/40 px-3 py-4 text-sm text-foreground">
        No briefing available.
      </div>
    );
  }
  return (
    <div className="space-y-3 text-foreground">
      <div className="flex items-center gap-2">
        <Badge variant={briefing.from_cache ? "secondary" : "outline"}>
          {briefing.from_cache ? "Cached" : "Fresh"}
        </Badge>
        <span className="text-2xs text-muted-foreground">
          {formatDate(briefing.generated_at)}
        </span>
      </div>
      {/* Reader-first: render the markdown body as plain wrapped text rather
          than pulling in a markdown renderer the app doesn't already ship. */}
      <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-foreground">
        {briefing.body_markdown.trim() || "No briefing content."}
      </p>
      {briefing.citations && briefing.citations.length > 0 ? (
        <div className="space-y-1.5 rounded-md border border-border bg-muted/30 p-3">
          <h4 className="text-xs font-medium">Sources</h4>
          <ul className="space-y-1">
            {briefing.citations.slice(0, 12).map((citation, index) => (
              <li
                key={citation.message_id ?? index}
                className="truncate text-2xs text-muted-foreground"
                title={citation.subject ?? citation.message_id}
              >
                {citation.subject?.trim() || citation.message_id || "(source)"}
                {citation.date ? ` · ${formatShortDate(citation.date)}` : ""}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

function extractBriefing(value: unknown): Briefing | null {
  if (!isRecord(value)) return null;
  const candidate = isRecord(value.briefing) ? value.briefing : value;
  if (typeof candidate.body_markdown !== "string") return null;
  return candidate as unknown as Briefing;
}

function styleSummary(score: number): string {
  if (score >= 0.68) return `formal (${score.toFixed(2)})`;
  if (score <= 0.38) return `casual (${score.toFixed(2)})`;
  return `neutral (${score.toFixed(2)})`;
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

function extractCommitments(value: unknown): Commitment[] {
  if (Array.isArray(value)) return value.filter(isCommitment);
  if (!isRecord(value)) return [];
  const commitments = value.commitments;
  return Array.isArray(commitments) ? commitments.filter(isCommitment) : [];
}

function isCommitment(value: unknown): value is Commitment {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.what === "string" &&
    typeof value.who_owes === "string" &&
    typeof value.direction === "string"
  );
}

function removeCommitmentFromSenderPayload(payload: unknown, commitmentId: string): unknown {
  if (!isRecord(payload)) return payload;
  const profile = isRecord(payload.profile) ? payload.profile : payload;
  const relationship = isRecord(profile.relationship) ? profile.relationship : null;
  const openCommitments = relationship?.open_commitments;
  if (!relationship || !Array.isArray(openCommitments)) return payload;

  const nextProfile = {
    ...profile,
    relationship: {
      ...relationship,
      open_commitments: openCommitments.filter(
        (commitment) => !isRecord(commitment) || commitment.id !== commitmentId,
      ),
    },
  };
  return profile === payload ? nextProfile : { ...payload, profile: nextProfile };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isAttachmentView(value: unknown): value is AttachmentView {
  return (
    isRecord(value) && typeof value.filename === "string" && typeof value.mime_type === "string"
  );
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

function formatShortDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(date);
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
