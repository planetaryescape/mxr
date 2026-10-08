//! Archive: records built from mail, and the answer box over them.
//!
//! A record is one thing (a receipt, an order, a booking, a bill), not one
//! email. The daemon files records from rules and schema.org, keeps where
//! every field came from, answers a query with the field asked for, and
//! serves the ledger by month with its totals and facets ready to draw.
//! Mutations take `dry_run`, and a preview selects exactly what the real
//! call changes.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

use super::ArchiveAnswerData;

/// The words Archive teaches itself with (D118, the #284 copy). One table,
/// so the CLI, TUI, web and agents say the same thing.
pub mod archive_copy {
    pub const HEADER: &str = "Receipts, orders, bookings and documents. Ask for what you need.";
    pub const NEVER_HAD_ANY: &str = "Receipts, orders, bookings, bills and documents are filed here as records, one card per thing, not per email. Type what you remember, like \"lisbon booking\", and it answers with the field.";
    pub const ADD_ONE_KEYS: &str = "Press `T` on any email and pick Archive to file it yourself.";
    pub const ADD_ONE_CLI: &str = "`mxr records file MESSAGE_ID` files one yourself.";
    /// Archive has no done: records stay.
    pub const CLEAR_FOR_NOW: &str = "Records stay. Ask for one, or browse by month.";
    /// The full explanation `?` leads with. Nothing shows it unasked.
    pub const ABOUT: &str = "Archive keeps records built from your mail: one card per order, trip or bill, and it answers in the field you asked for, like a booking reference. Archiving an email in Gmail is a different thing, and the toast always says which one happened.";
    pub const LANDS_HERE: &str = "Receipts, orders, bookings, bills, tickets, contracts and warranties, filed from your mail without you sorting anything. Receipts that come every month or year show as subscriptions, with the next charge and what they cost a year.";
    pub const FIRST_RUN_LINE: &str = "Receipts, orders, bookings";
    /// The toast after a record is filed: never "Archived", which is the
    /// provider's word.
    pub const FILED: &str = "Filed in Archive.";
    pub const NOT_A_RECORD: &str = "Not a record. Removed from Archive; the email is untouched.";
    pub const RESTORED: &str = "Back in Archive.";

    /// "No record matches "lisbon booking". Searching all mail instead."
    pub fn no_match(query: &str) -> String {
        format!("No record matches \"{query}\". Searching all mail instead.")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RecordKindData {
    Receipt,
    Order,
    Booking,
    Invoice,
    Statement,
    Ticket,
    Contract,
    Warranty,
    Account,
}

impl RecordKindData {
    pub const ALL: [Self; 9] = [
        Self::Receipt,
        Self::Order,
        Self::Booking,
        Self::Invoice,
        Self::Statement,
        Self::Ticket,
        Self::Contract,
        Self::Warranty,
        Self::Account,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Order => "order",
            Self::Booking => "booking",
            Self::Invoice => "invoice",
            Self::Statement => "statement",
            Self::Ticket => "ticket",
            Self::Contract => "contract",
            Self::Warranty => "warranty",
            Self::Account => "account",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        let value = value.strip_suffix('s').unwrap_or(&value);
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordAmountData {
    /// In the currency's minor unit.
    pub minor: i64,
    /// ISO 4217.
    pub currency: String,
    /// "£1,249.00".
    pub display: String,
}

/// One field of a record and where its value came from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordFieldData {
    /// issuer, title, reference, amount, issued_at, span_start, span_end,
    /// place, delivered_at, return_by, warranty_until, valid_until, kind
    pub field: String,
    /// "Booking ref", "Paid", "Date".
    pub label: String,
    /// As shown: "£1,249.00", "3 Mar 2025", "K7QX2M".
    pub value: String,
    /// What `y` copies: the reference as written, or the amount as a plain
    /// number ("1249.00").
    pub copy: String,
    /// schema | rule | delivery | todo | user
    pub source: String,
    /// "schema.org markup", "a pattern in the email", "you".
    pub source_label: String,
    /// False for money or a date nobody has confirmed. Shown with an open
    /// dot.
    pub checked: bool,
    /// The words it was read from, verbatim, or how it was worked out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// The email it was read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
}

/// A file attached to one of the record's emails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordDocumentData {
    pub message_id: MessageId,
    pub attachment_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub is_pdf: bool,
    /// Already downloaded, so it opens offline.
    pub on_disk: bool,
}

/// One email a record was built from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordSourceData {
    pub message_id: MessageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
    /// confirmation | shipped | delivered | return | refund | invoice |
    /// statement | booking | change | cancellation | receipt | other
    pub stage: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub subject: String,
    /// detector | delivery | todo | manual | sender
    pub filed_by: String,
}

/// The trip or series a record belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordGroupData {
    pub id: String,
    /// trip | series
    pub kind: String,
    /// "Lisbon, June 2025", "Octopus Energy bills".
    pub title: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_start: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_end: Option<chrono::DateTime<chrono::Utc>>,
}

/// One record: a ledger row, and with `GetRecord` the whole card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordData {
    pub id: String,
    pub account_id: AccountId,
    pub kind: RecordKindData,
    /// "Order", "Booking".
    pub kind_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// What it is: "XPS 14 laptop", "LHR -> LIS TP1357". Never the subject
    /// line when better is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// "Order", "Booking ref": what the reference is called on this kind.
    pub reference_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<RecordAmountData>,
    /// The ledger date: the transaction's, else the newest email's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_start: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_end: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_by: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warranty_until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    /// Every money and date field came from schema.org or you.
    pub checked: bool,
    /// The money and date fields still to confirm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unchecked_fields: Vec<String>,
    /// An order's stages in one line: "ordered · shipped · delivered 7 Mar".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_line: Option<String>,
    /// "Return by 2 Apr (passed) · Warranty to 3 Mar 2027".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_line: Option<String>,
    /// The record's best document, a PDF first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf: Option<RecordDocumentData>,
    pub document_count: u32,
    pub source_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<RecordGroupData>,
    /// "Here because: order confirmation with schema.org markup (checked)."
    pub why: String,
    /// schema | rule | delivery | todo | manual | sender
    pub origin: String,
    /// The newest source email's thread, for "open the email".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
    /// The newest source email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    #[serde(default)]
    pub dismissed: bool,
    /// Every field with its provenance.
    #[serde(default)]
    pub fields: Vec<RecordFieldData>,
    /// The card only (`GetRecord`): every file and every source email.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<RecordDocumentData>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<RecordSourceData>,
    /// The card only: how many other records this issuer has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_records: Option<u32>,
}

/// Which records a list or an export covers. Every condition is optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordFilterData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<RecordKindData>,
    /// An issuer name, matched without case: the issuer page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// In minor units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amount_minor: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_amount_minor: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_pdf: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// A trip or series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

/// One month of the ledger, with its count and totals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordMonthData {
    /// "2025-03".
    pub month: String,
    /// "2025 · March".
    pub label: String,
    pub count: u32,
    /// One total per currency, largest first. Never converted.
    pub totals: Vec<RecordAmountData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordFacetCountData {
    /// What to filter by: "order", "dell", "2025".
    pub value: String,
    pub label: String,
    pub count: u32,
}

/// Counts for each filter value over the matching records.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordFacetsData {
    pub kinds: Vec<RecordFacetCountData>,
    /// The thirty issuers with most records.
    pub issuers: Vec<RecordFacetCountData>,
    /// Newest first.
    pub years: Vec<RecordFacetCountData>,
    pub has_pdf: u32,
    pub checked: u32,
    pub unchecked: u32,
}

/// A record with a moment coming up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordMomentData {
    /// trip | booking | ticket | return_window | warranty, or a
    /// subscription's price_change | missed_charge | renewal_approaching
    /// (then `group_id` is the subscription's id and `record_id` its
    /// newest charge).
    pub kind: String,
    pub record_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    pub at: chrono::DateTime<chrono::Utc>,
    /// "Lisbon, June 2025 · in 3 days", "Return XPS 14 laptop by Wed".
    pub label: String,
}

/// The issuer page: every record from one issuer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordIssuerData {
    pub name: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<chrono::DateTime<chrono::Utc>>,
    pub totals: Vec<RecordAmountData>,
}

/// Where filing history has got to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordFirstRunData {
    pub complete: bool,
    pub scanned: u64,
    /// The oldest email read so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reached: Option<chrono::DateTime<chrono::Utc>>,
    /// "Filing your records. 418 found so far, back to March 2023."
    pub line: String,
}

/// Returned in `ResponseData::RecordLedger`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordLedgerData {
    pub header: String,
    /// Every record, whatever the filter.
    pub total: u32,
    /// Records the filter matches.
    pub matching: u32,
    /// This page of the matching records, newest first.
    pub records: Vec<RecordData>,
    /// Every month of the matching records, newest first, with counts and
    /// totals over all of them, not just this page.
    pub months: Vec<RecordMonthData>,
    pub facets: RecordFacetsData,
    /// Trips, tickets, return windows and warranties with a moment soon.
    pub coming_up: Vec<RecordMomentData>,
    pub filter: RecordFilterData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<RecordIssuerData>,
    /// Set when there is nothing to show: the never-had-any copy, or what
    /// the filter excluded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_run: Option<RecordFirstRunData>,
}

/// The field the answer box returns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordAnswerCardData {
    pub record: RecordData,
    /// reference | amount | issued_at | document
    pub field: String,
    /// "Booking ref".
    pub label: String,
    /// "K7QX2M".
    pub value: String,
    /// What `y` copies.
    pub copy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<RecordFieldData>,
}

/// What the answer box fell back to when no record matched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordFallbackData {
    /// "No record matches "warranty dell". Searching all mail instead."
    pub note: String,
    /// `mxr ask`'s citation-checked answer, or the retrieved emails when no
    /// model is set up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<ArchiveAnswerData>,
    /// Why the fallback has no answer, when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// How the answer box shows what matched.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RecordAnswerModeData {
    /// One record on the answer card: the query asked for a field, or one
    /// record won clearly.
    #[default]
    Answer,
    /// Every match: the query only named something several records match.
    List,
}

/// Every record a list-mode query matched, shaped like the ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordAnswerListData {
    /// "Anthropic · 23 records · £412.60".
    pub header: String,
    /// Every match, not just this page.
    pub count: u32,
    pub offset: u32,
    /// This page of the matches, newest first, as the ledger orders them.
    pub records: Vec<RecordData>,
    /// Every month of the matches, newest first, with counts and totals
    /// over all of them.
    pub months: Vec<RecordMonthData>,
    /// One total per currency over every match, largest first; never
    /// converted.
    pub totals: Vec<RecordAmountData>,
    /// The oldest and newest match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<chrono::DateTime<chrono::Utc>>,
    /// The best match, which the list highlights.
    pub top_record_id: String,
    /// Set when every match has this one issuer: its issuer page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
}

/// Returned in `ResponseData::RecordAnswer`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordAnswerData {
    pub query: String,
    /// reference | amount | date | document | any
    pub asked: String,
    #[serde(default)]
    pub mode: RecordAnswerModeData,
    /// The best match's card. In list mode it is the record the list
    /// highlights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<RecordAnswerCardData>,
    /// Other records that matched, best first: "Also matching".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<RecordData>,
    /// How many records matched in all: "Show all 23 matches".
    #[serde(default)]
    pub matching: u32,
    /// Every match, in list mode. Boxed so the answer stays small next to
    /// the other `ResponseData` variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<Box<RecordAnswerListData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<RecordFallbackData>,
}

/// A correction to a record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RecordEditData {
    /// Your value for a field: text, an amount ("£12.50", "12.50 EUR") or a
    /// date ("3 March 2025", "2025-03-03"). It wins over every extracted
    /// value, now and on every re-run.
    Set { field: String, value: String },
    /// The extracted value is right: it becomes yours, checked.
    Confirm { field: String },
    /// Confirm every unchecked money and date field: the card is checked.
    ConfirmAll,
    /// Drop your value, so the extracted one shows again.
    Clear { field: String },
}

/// How `u` reverses a record change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordUndoData {
    /// restore | dismiss | clear_fields | sender
    pub kind: String,
    pub record_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

/// Returned in `ResponseData::RecordChange`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordChangeData {
    pub dry_run: bool,
    /// file | dismiss | restore | set_field | sender
    pub action: String,
    /// The records changed, or that would change, as they are (or would
    /// be) after.
    pub records: Vec<RecordData>,
    /// The toast: "Filed in Archive.", never "Archived".
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<RecordUndoData>,
}

/// Returned in `ResponseData::RecordExport`. The dry run and the export
/// read the same rows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordExportData {
    pub dry_run: bool,
    pub rows: u32,
    /// One total per currency, largest first.
    pub totals: Vec<RecordAmountData>,
    /// Rows with an amount or date nobody has confirmed.
    pub unchecked: u32,
    /// Rows with no PDF.
    pub missing_pdfs: u32,
    pub by_kind: Vec<RecordFacetCountData>,
    /// "142 records, £18,204.11 and €612.00. 9 have an unchecked amount or
    /// date; 31 have no PDF."
    pub summary: String,
    /// The CSV, header first. Only on a real export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csv: Option<String>,
    /// Where PDFs were copied, when asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments_dir: Option<String>,
    /// PDFs copied there.
    #[serde(default)]
    pub pdfs_copied: u32,
    /// PDFs that could not be fetched or copied, by filename.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_errors: Vec<String>,
}
