//! Mail kinds and the places they live: Reading (newsletters and lists) and
//! Paper trail (receipts, notifications, automated mail), grouped by sender.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

/// A place for mail that isn't from people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MailPlaceData {
    /// Newsletters and mailing lists.
    Reading,
    /// Receipts, notifications and other automated mail.
    PaperTrail,
}

/// What kind of sender mail comes from, and so where it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SenderKindData {
    /// A person: their mail belongs on the desk.
    People,
    Reading,
    PaperTrail,
    /// Screened out: kept off the desk and every place.
    ScreenedOut,
}

/// Which rule decided a message's kind, most decisive first. `Decision`
/// means the user moved the sender; everything else is automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum KindRuleData {
    /// The user put this sender in a kind (a screener decision).
    Decision,
    /// The message is a delivery update.
    Delivery,
    /// The message carries a calendar invite.
    Invite,
    /// The address is a notifying machine's (`notifications@`, `receipts@`).
    AutomatedAddress,
    /// Sent from a notifying machine's subdomain (`alerts.example.com`).
    AutomatedDomain,
    /// The address is a newsletter's (`newsletter@`, `digest@`).
    NewsletterAddress,
    /// Sent from a newsletter's subdomain (`news.example.com`).
    NewsletterDomain,
    /// Carries a List-Id header.
    ListId,
    /// Carries a List-Unsubscribe header.
    ListUnsubscribe,
    /// A `no-reply@` address with no list headers: transactional mail.
    NoReplyAddress,
    /// A bank or card alert: an amount in the subject with what happened
    /// to it ("R437.77 reserved for purchase") or a masked card number.
    TransactionAlert,
    /// A role address (`forex@`, `hello@`, `support@`) you have never
    /// written to.
    RoleAddress,
    /// Sent through a bulk-mail service (SendGrid, Amazon SES and the
    /// like) by a sender you have never written to.
    BulkSender,
    /// A sender you have never written to whose latest subjects are a few
    /// templates repeated.
    TemplatedSender,
    /// The sender is known to write to lists.
    ListSender,
    /// None of the above: a person.
    Person,
    /// A person wrote, but you were only copied (or it went to a crowd)
    /// and it isn't your turn: the thread is in Updates, not Messages.
    Copied,
}

/// A message's kind with the reason it was given.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MailKindData {
    pub kind: SenderKindData,
    pub rule: KindRuleData,
    /// A short human reason, e.g. "automated sender, has List-Unsubscribe".
    pub reason: String,
    /// True when the user chose this kind for the sender.
    pub corrected: bool,
}

/// One message inside a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlaceMessageData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub snippet: String,
    pub date: chrono::DateTime<chrono::Utc>,
    /// Shown for reference only: nothing in a place counts as unread.
    pub unread: bool,
    pub pinned: bool,
    pub starred: bool,
}

/// One sender's mail in a place.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlaceBundleData {
    pub account_id: AccountId,
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    /// Why this sender's mail is here: the newest message's kind.
    pub kind: MailKindData,
    pub message_count: u32,
    /// Unread messages in the bundle. Never a badge; shown only as a fact.
    pub unread_count: u32,
    pub pinned_count: u32,
    pub newest_at: chrono::DateTime<chrono::Utc>,
    pub newest_subject: String,
    /// Pinned first, then newest: the page the request's `message_offset`
    /// and `messages_per_bundle` ask for. `message_count` covers them all.
    pub messages: Vec<PlaceMessageData>,
}

/// One sender a sweep would archive mail from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SweepSenderData {
    pub account_id: AccountId,
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub count: u32,
}

/// What a sweep archives. The dry run and the real sweep build this from
/// the same selection; the real sweep only ever narrows the preview's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SweepPreviewData {
    pub place: MailPlaceData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_email: Option<String>,
    /// Messages that will be (or were) archived.
    pub count: u32,
    /// Pinned messages left where they are.
    pub pinned_excluded: u32,
    /// Largest first.
    pub senders: Vec<SweepSenderData>,
    /// Up to five subjects, newest first.
    pub sample_subjects: Vec<String>,
    /// Hands the previewed selection to the real sweep, which archives
    /// only those messages. Expires after a few minutes and works once.
    /// Absent when nothing would be archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_token: Option<String>,
}
