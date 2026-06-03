export interface MessageRowView {
  id: string;
  kind: "message" | "thread" | string;
  account_id?: string;
  thread_id: string;
  provider_id: string;
  sender: string;
  sender_detail?: string | null;
  subject: string;
  snippet: string;
  date: string;
  date_label: string;
  date_full: string;
  date_relative: string;
  to?: AddressView[];
  cc?: AddressView[];
  bcc?: AddressView[];
  labels?: MessageLabelView[];
  unread: boolean;
  starred: boolean;
  has_attachments: boolean;
  /// Tri-state link-density classification computed at sync time. Drives the
  /// `🔗` indicator next to the subject. Absent for older payloads — treat as
  /// `"none"` when missing.
  link_density?: "none" | "some" | "heavy";
  message_count?: number | null;
  attachment_id?: string | null;
  attachment_filename?: string | null;
  attachment_size_bytes?: number | null;
  open_commitment_count?: number | null;
  triage_verdict?: "ACTION" | "FYI" | "ROUTINE" | string | null;
  triage_reason?: string | null;
  triage_line?: string | null;
}

export interface MessageLabelView {
  id: string;
  name: string;
  kind: "system" | "folder" | "user" | string;
  color?: string | null;
}

export interface MessageGroupView {
  id: string;
  label: string;
  rows: MessageRowView[];
}

export interface MailboxCounts {
  unread?: number;
  total?: number;
}

export interface SidebarLens {
  kind: "inbox" | "all_mail" | "label" | "saved_search" | "subscription" | string;
  labelId?: string | null;
  savedSearch?: string | null;
  senderEmail?: string | null;
}

export interface SidebarItem {
  id: string;
  label: string;
  unread?: number;
  total?: number;
  active?: boolean;
  lens?: SidebarLens;
}

export interface SidebarSection {
  id: string;
  title: string;
  items: SidebarItem[];
}

export interface ShellData {
  accountLabel?: string;
  syncLabel?: string;
  statusMessage?: string;
  commandHint?: string;
}

export interface ShellResponse {
  shell?: ShellData;
  sidebar?: { sections?: SidebarSection[] };
}

export interface MailboxResponse extends ShellResponse {
  mailbox: {
    lensLabel: string;
    view: "threads" | "messages" | string;
    counts: MailboxCounts;
    has_more?: boolean;
    next_offset?: number | null;
    groups: MessageGroupView[];
  };
}

export interface AddressView {
  name?: string | null;
  email: string;
}

export interface AttachmentView {
  id?: string;
  message_id?: string;
  part_id?: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  content_id?: string | null;
  local_path?: string | null;
  provider_id?: string | null;
}

export interface MessageBodyView {
  message_id: string;
  text_plain?: string | null;
  text_html?: string | null;
  reader_text?: string | null;
  attachments?: AttachmentView[];
  metadata?: MessageMetadataView;
}

export type CalendarPartstatView =
  | "needs_action"
  | "accepted"
  | "tentative"
  | "declined"
  | "delegated";

export interface CalendarPersonView {
  email: string;
  name?: string | null;
  uri?: string | null;
}

export interface CalendarAttendeeView extends CalendarPersonView {
  partstat?: string | null;
  role?: string | null;
  rsvp?: boolean | null;
}

export interface CalendarMetadataView {
  method?: string | null;
  summary?: string | null;
  component_kind?: string | null;
  uid?: string | null;
  sequence?: number | null;
  recurrence_id?: string | null;
  dtstamp?: string | null;
  starts_at?: string | null;
  ends_at?: string | null;
  description?: string | null;
  location?: string | null;
  status?: string | null;
  rrule?: string | null;
  organizer?: CalendarPersonView | null;
  attendees?: CalendarAttendeeView[];
  rsvp_requested?: boolean;
  raw_ics?: string | null;
  warnings?: string[];
  viewer_partstat?: CalendarPartstatView | null;
  viewer_attendee_email?: string | null;
  is_update?: boolean;
}

export interface MessageMetadataView {
  calendar?: CalendarMetadataView | null;
  list_id?: string | null;
  auth_results?: string | null;
  content_language?: string[] | null;
  text_plain_format?: unknown;
  text_plain_source?: string | null;
  text_html_source?: string | null;
  raw_headers?: string | null;
}

export interface ThreadView {
  account_id: string;
  id: string;
  latest_date: string;
  message_count: number;
  participants: AddressView[];
  snippet: string;
  subject: string;
  unread_count: number;
}

export interface ThreadSummaryViewData {
  text: string;
  model?: string | null;
  generated_at?: string | null;
}

export interface ThreadResponse {
  thread: ThreadView;
  messages: MessageRowView[];
  bodies: MessageBodyView[];
  body_failures?: unknown[];
  summary?: ThreadSummaryViewData | null;
  reader_mode?: string | null;
  right_rail?: { title?: string; items?: string[] };
}

export interface AccountMutationResult {
  account_id: string;
  account_name: string;
  succeeded: number;
  skipped: number;
  failed: number;
  error?: string | null;
}

export interface MutationResult {
  requested: number;
  succeeded: number;
  skipped: number;
  failed: number;
  accounts?: AccountMutationResult[];
  mutation_id?: string;
}

export interface MutationResponse {
  ok: boolean;
  result?: MutationResult;
}
