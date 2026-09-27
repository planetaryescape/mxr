/*
 * Shapes of the payloads the right rail is opened with, and the guards that
 * read them. Payloads arrive as `unknown` from the modal store, so every
 * panel narrows through these before rendering.
 */

import type { AttachmentView } from "@/features/mailbox/types";

export function isThreadContext(value: unknown): value is { title?: string; items?: string[] } {
  return typeof value === "object" && value !== null && "items" in value;
}

interface DraftAssistPayload {
  threadId: string;
  /** The message a reply would answer. */
  messageId?: string;
}

export function isDraftAssistPayload(value: unknown): value is DraftAssistPayload {
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

export interface RelationshipProfile {
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

interface Briefing {
  thread_id: string;
  body_markdown: string;
  citations?: { message_id?: string; subject?: string; date?: string }[];
  generated_at: string;
  from_cache: boolean;
}

export function extractBriefing(value: unknown): Briefing | null {
  if (!isRecord(value)) return null;
  const candidate = isRecord(value.briefing) ? value.briefing : value;
  if (typeof candidate.body_markdown !== "string") return null;
  return candidate as unknown as Briefing;
}

export function extractSenderProfile(value: unknown): SenderProfile | null {
  if (!isRecord(value)) return null;
  const candidate = isRecord(value.profile) ? value.profile : value;
  if (typeof candidate.email !== "string") return null;
  return candidate as unknown as SenderProfile;
}

export function extractCommitments(value: unknown): Commitment[] {
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

export function removeCommitmentFromSenderPayload(payload: unknown, commitmentId: string): unknown {
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

export function isAttachmentView(value: unknown): value is AttachmentView {
  return (
    isRecord(value) && typeof value.filename === "string" && typeof value.mime_type === "string"
  );
}
