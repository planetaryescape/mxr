use mxr_core::SearchMode;
use mxr_protocol::LlmConfigData;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(super) struct MailboxQuery {
    #[serde(default = "default_limit")]
    pub(super) limit: u32,
    #[serde(default)]
    pub(super) offset: u32,
    #[serde(default)]
    pub(super) view: MailboxView,
    #[serde(default)]
    pub(super) lens_kind: MailboxLensKind,
    #[serde(default)]
    pub(super) label_id: Option<String>,
    #[serde(default)]
    pub(super) saved_search: Option<String>,
    #[serde(default)]
    pub(super) sender_email: Option<String>,
    #[serde(default)]
    pub(super) token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MailboxView {
    #[default]
    Threads,
    Messages,
}

impl MailboxView {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Threads => "threads",
            Self::Messages => "messages",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MailboxLensKind {
    #[default]
    Inbox,
    AllMail,
    Label,
    SavedSearch,
    Subscription,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct MailboxLensRequest {
    pub(super) kind: MailboxLensKind,
    pub(super) label_id: Option<String>,
    pub(super) saved_search: Option<String>,
    pub(super) sender_email: Option<String>,
}

impl MailboxQuery {
    pub(super) fn lens(&self) -> MailboxLensRequest {
        MailboxLensRequest {
            kind: self.lens_kind.clone(),
            label_id: self.label_id.clone(),
            saved_search: self.saved_search.clone(),
            sender_email: self.sender_email.clone(),
        }
    }
}

fn default_limit() -> u32 {
    200
}

#[derive(Debug, Deserialize)]
pub(super) struct SearchQuery {
    #[serde(default)]
    pub(super) q: String,
    #[serde(default = "default_limit")]
    pub(super) limit: u32,
    #[serde(default)]
    pub(super) offset: u32,
    #[serde(default)]
    pub(super) mode: Option<SearchMode>,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) sort: Option<String>,
    #[serde(default)]
    pub(super) explain: bool,
    #[serde(default)]
    pub(super) token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MessageIdsRequest {
    pub(super) message_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StarRequest {
    pub(super) message_ids: Vec<String>,
    pub(super) starred: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct ReadRequest {
    pub(super) message_ids: Vec<String>,
    pub(super) read: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ComposeSessionKindRequest {
    New,
    Reply,
    ReplyAll,
    Forward,
    /// "Reply with comment" path for a calendar invite. Triggers the daemon's
    /// `Request::PrepareInviteResponse` and seeds the draft with the inline
    /// REPLY ICS so the outbound builder emits the correct MIME layout.
    InviteReply,
}

#[derive(Debug, Deserialize)]
pub(super) struct ComposeSessionStartRequest {
    pub(super) kind: ComposeSessionKindRequest,
    #[serde(default)]
    pub(super) message_id: Option<String>,
    #[serde(default)]
    pub(super) to: Option<String>,
    /// Required when `kind == InviteReply`. One of `accept`, `tentative`,
    /// `decline`. Ignored for other kinds.
    #[serde(default)]
    pub(super) action: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ComposeSessionPathRequest {
    pub(super) draft_path: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ComposeSessionRestoreRequest {
    pub(super) draft_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ComposeSessionUpdateRequest {
    pub(super) draft_path: String,
    pub(super) to: String,
    pub(super) cc: String,
    pub(super) bcc: String,
    pub(super) subject: String,
    pub(super) from: String,
    #[serde(default)]
    pub(super) attach: Vec<String>,
    #[serde(default)]
    pub(super) body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ComposeSessionSendRequest {
    pub(super) draft_path: String,
    pub(super) account_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ComposeSessionAttachmentRequest {
    pub(super) draft_path: String,
    pub(super) filename: String,
    pub(super) content_base64: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ModifyLabelsRequest {
    pub(super) message_ids: Vec<String>,
    #[serde(default)]
    pub(super) add: Vec<String>,
    #[serde(default)]
    pub(super) remove: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MoveRequest {
    pub(super) message_ids: Vec<String>,
    pub(super) target_label: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct RouteRequest {
    pub(super) message_ids: Vec<String>,
    pub(super) to_label: String,
    pub(super) from_queue_label: String,
    #[serde(default)]
    pub(super) archive: bool,
    #[serde(default)]
    pub(super) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct RuleQuery {
    pub(super) rule: String,
    #[serde(default)]
    pub(super) token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DeleteRuleRequest {
    pub(super) rule: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpsertRuleRequest {
    pub(super) rule: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpsertRuleFormRequest {
    pub(super) existing_rule: Option<String>,
    pub(super) name: String,
    pub(super) condition: String,
    pub(super) action: String,
    pub(super) priority: i32,
    pub(super) enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct SetDefaultAccountRequest {
    pub(super) key: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct StartAuthSessionRequest {
    pub(super) account: mxr_protocol::AccountConfigData,
    #[serde(default)]
    pub(super) reauthorize: bool,
    #[serde(default)]
    pub(super) flow: mxr_protocol::AuthFlowData,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompleteAuthSessionRequest {
    #[serde(default)]
    pub(super) save_account: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct AttachmentRequest {
    pub(super) message_id: String,
    pub(super) attachment_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct UnsubscribeRequest {
    pub(super) message_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct SnoozeRequest {
    pub(super) message_id: String,
    pub(super) until: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct InviteReplyRequest {
    pub(super) message_id: String,
    pub(super) action: mxr_protocol::CalendarInviteActionData,
    #[serde(default)]
    pub(super) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct LlmConfigRequest {
    pub(super) enabled: bool,
    pub(super) base_url: String,
    pub(super) model: String,
    pub(super) api_key_env: String,
    pub(super) context_window: u32,
    pub(super) request_timeout_secs: u64,
    #[serde(default)]
    pub(super) allow_cloud_relationship_data: bool,
    #[serde(default)]
    pub(super) overrides: Option<mxr_protocol::LlmOverridesData>,
}

impl From<LlmConfigRequest> for LlmConfigData {
    fn from(value: LlmConfigRequest) -> Self {
        Self {
            enabled: value.enabled,
            base_url: value.base_url,
            model: value.model,
            api_key_env: value.api_key_env,
            context_window: value.context_window,
            request_timeout_secs: value.request_timeout_secs,
            allow_cloud_relationship_data: value.allow_cloud_relationship_data,
            overrides: value.overrides,
        }
    }
}
