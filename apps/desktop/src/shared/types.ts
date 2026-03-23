export interface MxrStatusSnapshot {
  protocol_version: number;
  daemon_version: string | null;
  daemon_build_id?: string | null;
}

export interface BridgeReadyState {
  kind: "ready";
  baseUrl: string;
  authToken: string;
  binaryPath: string;
  usingBundled: boolean;
  daemonVersion: string | null;
  protocolVersion: number;
}

export interface BridgeMismatchState {
  kind: "mismatch";
  binaryPath: string;
  usingBundled: boolean;
  daemonVersion: string | null;
  actualProtocol: number | null;
  requiredProtocol: number;
  updateSteps: string[];
  detail: string;
}

export interface BridgeErrorState {
  kind: "error";
  binaryPath: string;
  usingBundled: boolean;
  title: string;
  detail: string;
}

export interface BridgeIdleState {
  kind: "idle";
}

export type BridgeState =
  | BridgeReadyState
  | BridgeMismatchState
  | BridgeErrorState
  | BridgeIdleState;

export interface DesktopApi {
  getBridgeState(): Promise<BridgeState>;
  retryBridge(): Promise<BridgeState>;
  useBundledMxr(): Promise<BridgeState>;
  setExternalBinaryPath(path: string): Promise<BridgeState>;
  openDraftInEditor(request: OpenDraftInEditorRequest): Promise<ActionAckResponse>;
  openExternalUrl(url: string): Promise<ActionAckResponse>;
  openLocalPath(path: string): Promise<ActionAckResponse>;
  openConfigFile(): Promise<ActionAckResponse>;
}

export type WorkbenchScreen = "mailbox" | "search" | "rules" | "accounts" | "diagnostics";

export type LayoutMode = "twoPane" | "threePane" | "fullScreen";

export type FocusContext =
  | "sidebar"
  | "mailList"
  | "reader"
  | "search"
  | "commandPalette"
  | "compose"
  | "dialog";

export type ReaderMode = "auto" | "reader" | "html" | "raw";

export type SearchScope = "threads" | "messages" | "attachments";

export type SearchSort = "relevant" | "recent";

export type SearchMode = "lexical" | "hybrid" | "semantic";

export interface WorkbenchShellPayload {
  accountLabel: string;
  syncLabel: string;
  statusMessage: string;
  commandHint: string;
}

export interface SidebarItem {
  id: string;
  label: string;
  unread: number;
  total: number;
  active: boolean;
  lens: SidebarLens;
}

export type SidebarLensKind = "inbox" | "all_mail" | "label" | "saved_search" | "subscription";

export interface SidebarLens {
  kind: SidebarLensKind;
  labelId?: string;
  savedSearch?: string;
  senderEmail?: string;
}

export interface SidebarSection {
  id: string;
  title: string;
  items: SidebarItem[];
}

export interface SidebarPayload {
  sections: SidebarSection[];
}

export interface MailboxCounts {
  unread: number;
  total: number;
}

export interface MailboxRow {
  id: string;
  thread_id: string;
  provider_id: string;
  sender: string;
  sender_detail?: string | null;
  subject: string;
  snippet: string;
  date_label: string;
  unread: boolean;
  starred: boolean;
  has_attachments: boolean;
}

export interface MailboxGroup {
  id: string;
  label: string;
  rows: MailboxRow[];
}

export interface MailboxPayload {
  lensLabel: string;
  counts: MailboxCounts;
  groups: MailboxGroup[];
}

export interface MailboxResponse {
  mailbox: MailboxPayload;
  sidebar: SidebarPayload;
  shell: WorkbenchShellPayload;
}

export interface ThreadSummary {
  id: string;
  subject: string;
  snippet: string;
}

export interface ThreadBody {
  message_id: string;
  text_plain?: string | null;
  text_html?: string | null;
  raw_source?: string | null;
  attachments: AttachmentMeta[];
}

export interface AttachmentMeta {
  id: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  local_path?: string | null;
}

export interface UtilityRailPayload {
  title: string;
  items: string[];
}

export interface ThreadResponse {
  thread: ThreadSummary;
  messages: MailboxRow[];
  bodies: ThreadBody[];
  reader_mode: ReaderMode;
  right_rail?: UtilityRailPayload;
}

export interface SearchResponse {
  scope: SearchScope;
  sort: SearchSort;
  mode: SearchMode;
  total: number;
  groups: MailboxGroup[];
  explain: unknown | null;
}

export interface ShellResponse {
  shell: WorkbenchShellPayload;
  sidebar: SidebarPayload;
}

export interface RulesResponse {
  rules: Array<Record<string, unknown>>;
}

export interface RuleDetailResponse {
  rule: Record<string, unknown>;
}

export interface RuleHistoryResponse {
  entries: Array<Record<string, unknown>>;
}

export interface RuleDryRunResponse {
  results: Array<Record<string, unknown>>;
}

export interface RuleFormPayload {
  id?: string | null;
  name: string;
  condition: string;
  action: string;
  priority: number;
  enabled: boolean;
}

export interface RuleFormResponse {
  form: RuleFormPayload;
}

export interface AccountSummary {
  account_id: string;
  key?: string | null;
  name: string;
  email: string;
  provider_kind: string;
  sync_kind?: string | null;
  send_kind?: string | null;
  enabled: boolean;
  is_default: boolean;
  source: string;
  editable: string;
  sync?: AccountSyncConfig | null;
  send?: AccountSendConfig | null;
}

export interface AccountsResponse {
  accounts: AccountSummary[];
}

export interface AccountConfig {
  key: string;
  name: string;
  email: string;
  sync?: AccountSyncConfig | null;
  send?: AccountSendConfig | null;
  is_default: boolean;
}

export type GmailCredentialSource = "bundled" | "custom";

export type AccountSyncConfig =
  | {
      type: "gmail";
      credential_source?: GmailCredentialSource;
      client_id: string;
      client_secret?: string | null;
      token_ref: string;
    }
  | {
      type: "imap";
      host: string;
      port: number;
      username: string;
      password_ref: string;
      password?: string | null;
      use_tls: boolean;
    };

export type AccountSendConfig =
  | {
      type: "gmail";
    }
  | {
      type: "smtp";
      host: string;
      port: number;
      username: string;
      password_ref: string;
      password?: string | null;
      use_tls: boolean;
    };

export interface AccountOperationStep {
  ok: boolean;
  detail: string;
}

export interface AccountOperationResult {
  ok: boolean;
  summary: string;
  save?: AccountOperationStep | null;
  auth?: AccountOperationStep | null;
  sync?: AccountOperationStep | null;
  send?: AccountOperationStep | null;
}

export interface AccountOperationResponse {
  result: AccountOperationResult;
}

export interface DiagnosticsReport {
  healthy: boolean;
  health_class: string;
  daemon_protocol_version: number;
  daemon_version?: string | null;
  daemon_build_id?: string | null;
  lexical_index_freshness: string;
  semantic_index_freshness: string;
  database_path?: string;
  index_path?: string;
  log_path?: string;
  log_size_bytes?: number;
  recommended_next_steps: string[];
  recent_error_logs: string[];
}

export interface DiagnosticsResponse {
  report: DiagnosticsReport;
}

export interface EmptyStatePayload {
  title: string;
  detail: string;
  actionLabel: string;
}

export interface ComposeFrontmatter {
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  from: string;
  attach: string[];
  references: string[];
  in_reply_to?: string | null;
}

export interface ComposeIssue {
  severity: "error" | "warning";
  message: string;
}

export type ComposeSessionKind = "new" | "reply" | "reply_all" | "forward";

export interface ComposeSession {
  draftPath: string;
  rawContent: string;
  frontmatter: ComposeFrontmatter;
  bodyMarkdown: string;
  previewHtml: string;
  issues: ComposeIssue[];
  cursorLine?: number;
  accountId: string;
  kind: ComposeSessionKind;
  editorCommand: string;
}

export interface ComposeSessionResponse {
  session: ComposeSession;
}

export interface SnoozePreset {
  id: string;
  label: string;
  wakeAt: string;
}

export interface SnoozePresetsResponse {
  presets: SnoozePreset[];
}

export interface ActionAckResponse {
  ok: boolean;
  wake_at?: string;
}

export interface ExportThreadResponse {
  content: string;
}

export interface BugReportResponse {
  content: string;
}

export interface AttachmentFile {
  attachment_id: string;
  filename: string;
  path: string;
}

export interface AttachmentFileResponse {
  file: AttachmentFile;
}

export interface OpenDraftInEditorRequest {
  draftPath: string;
  editorCommand: string;
  cursorLine?: number;
}
