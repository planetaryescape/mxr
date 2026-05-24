use mxr_core::id::*;
use mxr_core::types::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderEmailReferenceData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    pub from_email: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub direction: String,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderSummaryData {
    pub account_id: AccountId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub sender_email: String,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_subject: String,
    pub latest_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderProfileData {
    pub account_id: AccountId,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_inbound_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outbound_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub replied_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_days_p50: Option<f64>,
    pub is_list_sender: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
    pub open_thread_count: u32,
    #[serde(default)]
    pub inbound_storage_bytes: u64,
    #[serde(default)]
    pub outbound_storage_bytes: u64,
    #[serde(default)]
    pub attachment_count: u32,
    #[serde(default)]
    pub attachment_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unanswered_question: Option<SenderUnansweredQuestionData>,
    #[serde(default)]
    pub response_histogram: Vec<ResponseTimeBucket>,
    #[serde(default)]
    pub weekly_activity: Vec<SenderWeeklyActivityData>,
    #[serde(default)]
    pub recent_messages: Vec<SenderEmailReferenceData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<RelationshipProfileData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderUnansweredQuestionData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub days_waiting: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderWeeklyActivityData {
    pub week_start: chrono::DateTime<chrono::Utc>,
    pub inbound_count: u32,
    pub outbound_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelationshipProfileData {
    pub account_id: AccountId,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ContactStyleData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ContactRelationshipSummaryData>,
    #[serde(default)]
    pub open_commitments: Vec<CommitmentData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift: Option<RelationshipDriftData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelationshipDriftData {
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ContactStyleData {
    pub formality_score: f64,
    pub formality_score_theirs: f64,
    pub avg_sentence_len: f64,
    pub avg_sentence_len_theirs: f64,
    pub msg_count_used: u32,
    pub msg_count_used_theirs: u32,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ContactRelationshipSummaryData {
    pub text: String,
    pub model: String,
    pub known_topics: Vec<String>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CommitmentDirectionData {
    Yours,
    Theirs,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CommitmentStatusData {
    Open,
    Resolved,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommitmentData {
    pub id: String,
    pub account_id: AccountId,
    pub email: String,
    pub thread_id: ThreadId,
    pub direction: CommitmentDirectionData,
    pub status: CommitmentStatusData,
    pub who_owes: String,
    pub what: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_when: Option<chrono::DateTime<chrono::Utc>>,
    pub evidence_msg_id: MessageId,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArchiveAskMode {
    #[default]
    Hybrid,
    Lexical,
    Semantic,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveAskFiltersData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<AccountId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub mode: ArchiveAskMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveCitationData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub quote: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveRetrievalData {
    pub requested_mode: ArchiveAskMode,
    pub executed_mode: ArchiveAskMode,
    pub candidate_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CitationRefData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub field: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WhoisCitationData {
    pub msg_id: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EntityCandidateData {
    pub kind: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub mention_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EntityExplanationData {
    pub canonical_name: String,
    pub kind: String, // "person" | "term" | "ambiguous" | "unknown"
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub topics: Vec<String>,
    pub citations: Vec<WhoisCitationData>,
    pub candidates: Vec<EntityCandidateData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExpertSuggestionData {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub reason: String,
    pub answered_thread_count: u32,
    pub evidence_msg_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SuggestedRecipientData {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub reason: String,
    pub confidence: String, // "low" | "medium" | "high"
    pub evidence_msg_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadBriefingData {
    /// thread_id (Slice 5.1) or recipient email (Slice 5.2).
    pub thread_id: String,
    pub body_markdown: String,
    pub citations: Vec<CitationRefData>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub from_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelationshipWatchEntryData {
    pub account_id: AccountId,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_days: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub added_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CadenceDriftRowData {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contact_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expected_days: f64,
    pub drift_days: f64,
    pub total_volume: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SendTimeConfidenceData {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SendTimeBucketData {
    pub weekday: u8,
    pub hour: u8,
    pub p50_seconds: i64,
    pub sample_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SendWindowData {
    pub weekday: u8,
    pub hour_start: u8,
    pub hour_end: u8,
    pub expected_reply_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecipientSendTimeRowData {
    pub email: String,
    pub sample_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_expected_reply_seconds: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_expected_reply_seconds: Option<i64>,
    pub best_windows: Vec<SendWindowData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SendTimeRecommendationData {
    /// Slot the caller asked us to evaluate, echoed back so JSON
    /// consumers don't have to rebuild it. Present only when the
    /// caller passed `proposed_at` on the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Weekday (0 = Monday) of `proposed_at` in UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_weekday: Option<u8>,
    /// Hour (0-23) of `proposed_at` in UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_hour: Option<u8>,
    pub recipient_rows: Vec<RecipientSendTimeRowData>,
    pub best_windows: Vec<SendWindowData>,
    pub confidence: SendTimeConfidenceData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DecisionLogEntryData {
    pub id: String,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub evidence_msg_ids: Vec<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveAnswerData {
    pub text: String,
    pub citations: Vec<ArchiveCitationData>,
    pub retrieval: ArchiveRetrievalData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OwedReplyRowData {
    pub thread_id: ThreadId,
    pub latest_inbound_msg_id: MessageId,
    pub from_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    pub subject: String,
    pub latest_inbound_at: chrono::DateTime<chrono::Utc>,
    pub waiting_days: f64,
    pub expected_days: f64,
    pub overdue_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftCommitmentCandidateData {
    pub id: String,
    pub draft_id: DraftId,
    pub account_id: AccountId,
    pub email: String,
    pub direction: CommitmentDirectionData,
    pub who_owes: String,
    pub what: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_when: Option<chrono::DateTime<chrono::Utc>>,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VoiceRegisterData {
    Casual,
    Neutral,
    Formal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftLengthHintData {
    Short,
    Medium,
    Long,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftRefineKnobsData {
    #[serde(default)]
    pub shorter: bool,
    #[serde(default)]
    pub warmer: bool,
    #[serde(default)]
    pub more_formal: bool,
    #[serde(default)]
    pub less_emoji: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserVoiceRegisterModeData {
    pub register: VoiceRegisterData,
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub exemplar_message_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserVoiceProfileData {
    pub account_id: AccountId,
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub msg_count_used: u32,
    pub register_modes: Vec<UserVoiceRegisterModeData>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VoiceMatchConfidenceData {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VoiceMatchData {
    pub score: f64,
    pub confidence: VoiceMatchConfidenceData,
    pub notable_deltas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HumanizerHitData {
    pub category: String,
    pub matched: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HumanizerReportSummaryData {
    pub score: u8,
    pub hits: Vec<HumanizerHitData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum FeatureHealth {
    Healthy,
    Degraded { reason: String },
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FeatureHealthReport {
    pub semantic: FeatureHealth,
    pub summarize: FeatureHealth,
    pub relationship_profile: FeatureHealth,
    pub commitments: FeatureHealth,
    pub draft_assist: FeatureHealth,
    pub voice_match: FeatureHealth,
    pub humanizer: FeatureHealth,
}
