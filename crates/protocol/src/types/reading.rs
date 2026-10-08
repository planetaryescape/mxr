//! Reading (blueprint 22, phase 5): newsletters as an edition you visit,
//! a reader, and a Later shelf.
//!
//! The unit is a readable item, not a message: a single-essay issue, each
//! link in a digest, or the article a teaser points to. An item is named by
//! its key, `<message id>:<index>`; index 0 is the issue itself.

use mxr_core::id::{AccountId, MessageId, ThreadId};
use serde::{Deserialize, Serialize};

/// Reading's words, from the blueprint's copy table (#284).
pub mod reading_copy {
    pub const HEADER: &str = "Newsletters you chose, as an edition. Read when you like.";
    pub const NEVER_HAD_ANY: &str = "Newsletters and posts you subscribed to land here, with the ones you read most first. Nothing here is owed: there's no unread count, and items fade unless you keep them.";
    /// Clear for now, before the "since" date and the Later count: "Nothing
    /// new since Tuesday. Later has 4 things saved."
    pub const CLEAR_FOR_NOW: &str = "Nothing new.";
    pub const ABOUT: &str = "Reading is an edition of the newsletters you chose, with the sources you read most first. Nothing here is owed: items fade after a while unless you press b to keep them for later.";
    pub const LANDS_HERE: &str =
        "Newsletters, digests and posts from writers: mail with an unsubscribe link that isn't a person or a notification.";
    pub const FIRST_RUN_LINE: &str = "Newsletters, when you like";
    /// The line between what's new and what you've seen.
    pub const LEFT_OFF: &str = "You left off here";
    /// The unsubscribe preview's last line.
    pub const UNSUBSCRIBE_IRREVERSIBLE: &str =
        "This can't be undone from mxr; you'd resubscribe on their site.";
    /// Asked once of an item on Later for over 30 days.
    pub const STILL_WANT_IT: &str = "Still want it?";
}

/// What an issue is, by the extractor's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReadingShapeData {
    /// One essay: the issue is the article.
    Single,
    /// Many links with blurbs: each link is its own item.
    Digest,
    /// A short body pointing at one article: the item is that article.
    Teaser,
    /// A few lines with nothing to follow.
    Notice,
}

/// An issue, or one link inside a digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReadingItemKindData {
    Issue,
    Link,
}

/// The edition's three time bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReadingBandData {
    /// Arrived after your previous visit.
    SinceLastVisit,
    /// Still inside its source's window.
    Earlier,
    /// In its last day.
    Fading,
}

/// How a source can be left: told directly, or a page to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReadingUnsubscribeData {
    /// RFC 8058: the sender is told directly, no browser.
    OneClick,
    /// A page the sender asks you to open.
    Link,
    /// An email to the list's address.
    Mailto,
    /// No method in the headers or the body.
    None,
}

impl From<&mxr_core::types::UnsubscribeMethod> for ReadingUnsubscribeData {
    fn from(method: &mxr_core::types::UnsubscribeMethod) -> Self {
        use mxr_core::types::UnsubscribeMethod;
        match method {
            UnsubscribeMethod::OneClick { .. } => Self::OneClick,
            UnsubscribeMethod::HttpLink { .. } | UnsubscribeMethod::BodyLink { .. } => Self::Link,
            UnsubscribeMethod::Mailto { .. } => Self::Mailto,
            UnsubscribeMethod::None => Self::None,
        }
    }
}

/// Where an item came from, and how you read it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingSourceData {
    pub account_id: AccountId,
    pub sender_email: String,
    /// The publication's name: the sender's display name, else the address.
    pub name: String,
    /// Recent issues the counts below cover (at most the last 20, 90 days).
    pub issues: u32,
    /// Of those, opened in mxr or read elsewhere.
    pub opened: u32,
    /// Of those, read to the end in mxr.
    pub finished: u32,
    /// "You opened 0 of the last 11 issues."
    pub evidence: String,
    /// Finished issues count three times an open, over issues sent.
    pub affinity: f64,
    /// How long its items stay: twice its usual gap, 2 to 14 days.
    pub window_days: f64,
    /// Its usual gap between issues, when it has sent two or more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_gap_days: Option<f64>,
    /// Three issues or fewer: it gets a lead slot.
    pub new_source: bool,
    /// You chose the sender's own layout for this source (`R`).
    pub original_layout: bool,
    pub unsubscribe: ReadingUnsubscribeData,
    /// Eight or more recent issues and none opened: offer to unsubscribe.
    pub suggest_unsubscribe: bool,
}

/// A link inside a digest, shown under its issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingLinkData {
    pub item_key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blurb: Option<String>,
    pub url: String,
    /// `sqlite.org`, or the click tracker's domain when `tracked`.
    pub domain: String,
    /// The link goes through a click tracker: the article's own site is
    /// only known once it is fetched.
    pub tracked: bool,
    pub on_later: bool,
    /// Its article is saved, so it reads offline.
    pub article_cached: bool,
}

/// One readable item in the edition or on Later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingItemData {
    /// `<message id>:<index>`; index 0 is the issue.
    pub item_key: String,
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub kind: ReadingItemKindData,
    /// The issue's shape (a link carries its issue's).
    pub shape: ReadingShapeData,
    /// The cleaned headline, or the link's title.
    pub title: String,
    /// The issue's first real paragraph, or the link's blurb.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standfirst: Option<String>,
    pub source: String,
    pub sender_email: String,
    /// Words in the issue's body; 0 for a link until its article is saved.
    pub words: u32,
    /// Minutes at your pace.
    pub minutes: u32,
    /// The article this item points at: a teaser's main link, a digest
    /// link's page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default)]
    pub tracked: bool,
    pub arrived_at: chrono::DateTime<chrono::Utc>,
    /// When it leaves the edition. Absent on Later, which never expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// "Here because: you subscribed, and it has an unsubscribe link
    /// (rule). Fades Sunday unless you keep it."
    pub why: String,
    /// "Fades Sunday unless you keep it."; absent on Later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fades: Option<String>,
    /// One of the top items, from a source you read most or a new one.
    #[serde(default)]
    pub lead: bool,
    /// "you read 9 of 10"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engagement: Option<String>,
    /// A digest's links, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ReadingLinkData>,
    pub on_later: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_at: Option<chrono::DateTime<chrono::Utc>>,
    /// On Later for over 30 days and not yet asked: "Still want it?"
    #[serde(default)]
    pub still_want_it: bool,
    /// How far you got, 0 to 1.
    pub progress: f64,
    pub opened: bool,
    pub finished: bool,
    pub article_cached: bool,
    /// The source's evidence when it suggests unsubscribing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsubscribe_offer: Option<String>,
}

/// One band of the edition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingBandGroupData {
    pub band: ReadingBandData,
    /// "Since you were last here".
    pub label: String,
    /// Fading: "Goes within a day, unless you keep it."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub items: Vec<ReadingItemData>,
}

/// The empty state, when the first band has nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingEmptyData {
    /// Nothing has ever landed in Reading.
    pub never_had_any: bool,
    /// "Nothing new since Tuesday. Later has 4 things saved."
    pub line: String,
}

/// Returned by `Request::GetReadingEdition`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingEditionData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub header: String,
    /// Only bands with items, in order.
    pub bands: Vec<ReadingBandGroupData>,
    /// Your previous visit ended here: "You left off here" goes after the
    /// first band when this is set and later bands have items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_visit_at: Option<chrono::DateTime<chrono::Utc>>,
    pub left_off_here: bool,
    /// The shelf, newest first. Its count is the only one Reading shows.
    pub later: Vec<ReadingItemData>,
    pub later_count: u32,
    /// Every source in the edition or on Later, best read first.
    pub sources: Vec<ReadingSourceData>,
    /// Your reading pace in words a minute, and whether it was measured
    /// from issues you finished (else 230).
    pub pace_wpm: u32,
    pub pace_measured: bool,
    /// Items that expired since the last edition: done in Reading, never
    /// archived at the provider.
    pub expired_now: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<ReadingEmptyData>,
}

/// One paragraph of reader text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingParagraphData {
    /// `heading`, `text`, `list_item` or `quote`.
    pub kind: String,
    pub text: String,
}

/// A fetched article, as cached.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingArticleData {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    /// Where the page was in the end.
    pub final_url: String,
    /// Every site contacted to fetch it, in order.
    pub contacted: Vec<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub words: u32,
    pub minutes: u32,
    pub paragraphs: Vec<ReadingParagraphData>,
    /// Cleaned article HTML; untrusted, so clients sanitize it.
    pub html: String,
}

/// A saved passage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingHighlightData {
    pub id: String,
    pub item_key: String,
    pub account_id: AccountId,
    pub message_id: MessageId,
    /// `issue` or `article`: which text it was taken from.
    pub view: String,
    pub quote: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The item's title and source, as of saving.
    pub title: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Returned by `Request::GetReadingItem`: the reader's page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingItemDetailData {
    pub item: ReadingItemData,
    pub source_data: ReadingSourceData,
    /// The issue as reader text, masthead and footer removed.
    pub paragraphs: Vec<ReadingParagraphData>,
    /// The issue as cleaned HTML when the cleaner kept most of it;
    /// untrusted, so clients sanitize it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// The saved article, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article: Option<ReadingArticleData>,
    /// Why the last fetch failed, kept so the reader can offer the browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article_error: Option<String>,
    pub highlights: Vec<ReadingHighlightData>,
    /// Minutes left at your pace, from your progress.
    pub minutes_left: u32,
    pub pace_wpm: u32,
}

/// What `SetReadingLater` did to one item, or would do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingLaterOutcomeData {
    pub item_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub on_later: bool,
    /// Whether this request changed it.
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What `FetchArticle` did.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadingFetchData {
    pub item_key: String,
    /// The site named before fetching: the link's own domain.
    pub domain: String,
    /// Served from the saved copy; nothing was contacted.
    pub cached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article: Option<ReadingArticleData>,
    /// Why it failed: a paywall, a private address, a page too large.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
