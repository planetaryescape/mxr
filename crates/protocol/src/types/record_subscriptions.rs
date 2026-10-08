//! Archive's subscriptions: receipts and invoices from one issuer for one
//! product that come at a steady cadence, with what they cost, when the
//! next charge is due and how the price moved. Not the newsletters of
//! `ListSubscriptions`, which are mail you subscribed to.
//!
//! Subscriptions are worked out from the records on every read, so they
//! carry no state of their own: correct a record and its subscription
//! follows.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

use super::{RecordAmountData, RecordFieldData};

/// The words the Subscriptions view teaches itself with.
pub mod subscription_copy {
    pub const HEADER: &str =
        "Receipts that come every week, month, quarter or year, with the next charge and what they cost.";
    pub const NONE_YET: &str = "No subscriptions yet. When three receipts from one issuer come a month apart (or two a year apart), they show here with the next charge and the yearly cost.";
    pub const ALL_ENDED: &str = "Nothing running now. The ones that ended are below.";
}

/// One charge of a subscription: a record, or a receipt and an invoice
/// for the same payment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordSubscriptionChargeData {
    pub record_ids: Vec<String>,
    pub date: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<RecordAmountData>,
    /// Its amount and date came from schema.org or you.
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordPriceChangeData {
    /// The first charge at the new price.
    pub date: chrono::DateTime<chrono::Utc>,
    pub record_id: String,
    pub from: RecordAmountData,
    pub to: RecordAmountData,
    /// "£10.99 to £12.99 on 10 May 2025".
    pub label: String,
}

/// One subscription: the row and, with its history, the card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordSubscriptionData {
    /// `sub_…`: stable while its first charge stays.
    pub id: String,
    pub account_id: AccountId,
    pub issuer: String,
    /// "Premium", when the receipts name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// "Spotify Premium", or the issuer alone.
    pub title: String,
    /// weekly | monthly | quarterly | yearly
    pub cadence: String,
    /// "Monthly".
    pub cadence_label: String,
    /// The newest charge's amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<RecordAmountData>,
    /// The amount times the charges in a year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yearly_cost: Option<RecordAmountData>,
    pub start: chrono::DateTime<chrono::Utc>,
    pub last_charge: chrono::DateTime<chrono::Utc>,
    /// Not set once it has ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_expected: Option<chrono::DateTime<chrono::Utc>>,
    /// active | overdue | ended
    pub status: String,
    /// "6 charges about a month apart since Jan 2025", "Expected around
    /// 3 May 2025; no charge since 3 Apr 2025", "Cancellation email on
    /// 20 Mar 2025".
    pub status_reason: String,
    /// Three charges or more. A yearly one seen twice is not yet.
    pub confirmed: bool,
    pub charge_count: u32,
    /// Oldest first.
    pub charges: Vec<RecordSubscriptionChargeData>,
    /// Oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub price_changes: Vec<RecordPriceChangeData>,
    /// Every field with where it came from: issuer, product, cadence,
    /// amount, yearly_cost, start, last_charge, next_expected, status.
    pub fields: Vec<RecordFieldData>,
    /// The issuer's receipts and invoices that are not part of any
    /// subscription.
    pub one_offs: u32,
    /// The newest charge's record, for "open the record".
    pub record_id: String,
    /// The newest charge's thread, for "open the email".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
    /// The newest charge's newest email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// "Here because: 6 receipts from Spotify about a month apart
    /// (worked out from your records)."
    pub why: String,
}

/// What the live subscriptions cost in one currency. Never converted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordSubscriptionTotalData {
    pub currency: String,
    /// The year over twelve.
    pub per_month: RecordAmountData,
    pub per_year: RecordAmountData,
}

/// Something about a subscription worth a look: a price change on the
/// newest charge, an expected charge that didn't come, or a yearly
/// subscription's next charge within its lead time. A suggestion only;
/// nothing here is ever filed as a to-do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordSubscriptionSignalData {
    /// price_change | missed_charge | renewal_approaching
    pub kind: String,
    pub subscription_id: String,
    /// The newest charge's record.
    pub record_id: String,
    pub at: chrono::DateTime<chrono::Utc>,
    /// "Netflix went up from £10.99 to £12.99 on 10 May",
    /// "No Spotify Premium charge since 3 Apr; expected around 3 May",
    /// "Admiral renews around 24 Oct".
    pub label: String,
}

/// Returned in `ResponseData::RecordSubscriptions`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecordSubscriptionsData {
    pub header: String,
    /// Live ones first (active and overdue), then ended, each by issuer.
    pub subscriptions: Vec<RecordSubscriptionData>,
    /// Over the live ones, per currency, largest first.
    pub totals: Vec<RecordSubscriptionTotalData>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<RecordSubscriptionSignalData>,
    pub live: u32,
    pub ended: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
}
