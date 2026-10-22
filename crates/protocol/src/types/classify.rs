//! `mxr classify eval`: today's classifier measured against labelled
//! scenarios (blueprint 32). Local only; reads stored mail and changes
//! nothing.

use serde::{Deserialize, Serialize};

/// Which labelled set to measure. `Mine` arrives with corrections
/// (blueprint 32, build order 5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ClassifyEvalSetData {
    /// The hand-written demo mail and the near-miss twins: the fast loop to
    /// run after every change.
    #[default]
    Quick,
    /// Every labelled email in the stored mail: on a demo, the hand-written
    /// mail and all of the generated bulk, which CI grades before merge.
    Full,
}

/// Accuracy on one question about an email. What an email asks of you (to-dos)
/// is not one of these: it is graded in [`ClassifyEvalAsksData`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalQuestionData {
    /// `who`, `home` or `record`.
    pub question: String,
    /// Labelled emails present in this mailbox and answerable (an email
    /// whose home cannot be worked out is not asked about its home).
    pub evaluated: u32,
    pub correct: u32,
    /// `correct / evaluated` as a percentage with one decimal; 0 when
    /// nothing was evaluated.
    pub accuracy_percent: f64,
}

/// One wrong answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalMismatchData {
    pub scenario: String,
    pub provider_id: String,
    pub question: String,
    pub expected: String,
    /// The answer today's code gave.
    pub actual: String,
    /// Why today's code answered so: the deciding rule's reason. Empty
    /// for to-do and record answers.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// The scenario this one is a near-miss twin of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twin_of: Option<String>,
    /// The email is worded in a phrasing held out of what agents see while
    /// changing a judge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub held_out: bool,
    /// Why the label itself is a judgement call, when it is: a mismatch
    /// here may be the label's fault, not the classifier's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalSummaryData {
    pub set: ClassifyEvalSetData,
    /// Version of the questions the expectations answer, so a change to
    /// them never silently invalidates old labels.
    pub label_schema_version: u32,
    /// Labelled emails in the set.
    pub labelled_in_set: u32,
    /// Of those, the ones found in this mailbox.
    pub present: u32,
    /// Present emails with no arrivals row (history older than 30 days when
    /// the account first synced), whose home was worked out live by the same
    /// rules.
    pub placed_live: u32,
    /// Present emails whose home could not be worked out at all, so there is
    /// none to compare.
    pub not_placed: u32,
    /// Stored emails in scope that no scenario labels.
    pub unlabelled: u32,
    pub questions: Vec<ClassifyEvalQuestionData>,
    /// Of the present emails, the ones worded in a held-out phrasing. A
    /// judge that learned the generator, not the mail, scores worse here
    /// than overall.
    #[serde(default)]
    pub held_out_present: u32,
    /// Accuracy on the held-out emails alone, per question. Empty when the
    /// set has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_out_questions: Vec<ClassifyEvalQuestionData>,
    /// What `home` reads: the arrivals line's current-policy destination.
    pub home_projection: String,
    /// What the emails ask of you, graded per kind of ask and per field
    /// (label schema v2). Absent when nothing was graded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asks: Option<ClassifyEvalAsksData>,
    /// A plain sentence when there is nothing to measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// How many items were expected, found and matched.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalCountsData {
    /// Items the labels expect.
    pub expected: u32,
    /// Items (to-do rows) the classifier made.
    pub found: u32,
    /// Found items that match an expected item by kind and direction.
    pub true_positives: u32,
    /// Found items that match no expected item.
    pub false_positives: u32,
    /// Expected items nothing matched.
    pub false_negatives: u32,
    /// `true_positives / found` as a percentage with one decimal; absent when
    /// nothing was found, because precision is then undefined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision_percent: Option<f64>,
    /// `true_positives / expected`; absent when nothing was expected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recall_percent: Option<f64>,
}

/// Precision and recall for one kind of ask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalAskKindData {
    /// A kind from blueprint 37: `pay`, `fix_payment`, `sign`, `verify`,
    /// `reply`, `send`, `provide`, `upload`, `book`, `attend`,
    /// `renew_or_cancel`, `register`, `contact` or `other`.
    pub kind: String,
    /// `asks_you` (inbound mail asks) or `you_promised` (your sent mail).
    pub direction: String,
    pub counts: ClassifyEvalCountsData,
}

/// How often one field of a matched item is exactly right.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalFieldData {
    /// `amount`, `due` or `counterparty`.
    pub field: String,
    /// Matched items: the ones this field is compared on.
    pub pairs: u32,
    /// Pairs where the row's field equals the label's, an absent value on
    /// both sides counting as equal. A due date is exact only when the row's
    /// day is the day the label names.
    pub exact: u32,
    pub exact_percent: f64,
    /// The label states a value and the row has none.
    pub missing: u32,
    /// Both have a value and they differ.
    pub wrong: u32,
    /// The label states a value the eval cannot resolve to an exact one, and
    /// the row has one. Counted apart and never as exact.
    #[serde(default)]
    pub present: u32,
    /// The label states none and the row has one.
    pub spurious: u32,
}

/// Twin pairs, where both members must be right on what they ask.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalTwinsData {
    /// Pairs with both members present.
    pub pairs: u32,
    pub both_right: u32,
    /// Of those, pairs whose members expect different asks (one asks
    /// something and the other nothing, or different kinds).
    pub differing_pairs: u32,
    pub differing_both_right: u32,
}

/// The ask results over one slice of the emails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalSliceData {
    /// `sender_kind`, `locale`, `template_family` or `phrasing`.
    pub dimension: String,
    /// For example `person`, `en-ZA`, `bill-due`, `held_out`.
    pub value: String,
    /// Labelled emails in the slice.
    pub emails: u32,
    pub counts: ClassifyEvalCountsData,
}

/// Whether a reply is owed, graded as its own question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalReplyData {
    /// Emails graded on it.
    pub emails: u32,
    /// `expected` emails owe a reply, `found` are marked as needing one.
    pub counts: ClassifyEvalCountsData,
    /// What the classifier's answer reads, so the number is not taken for
    /// more than it is.
    pub projection: String,
}

/// What the emails ask of you: to-do rows against the labelled items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalAsksData {
    /// Labelled emails graded on what they ask.
    pub emails: u32,
    /// All kinds together.
    pub total: ClassifyEvalCountsData,
    /// Per kind of ask. Every kind a person can be asked for is listed (with
    /// zero counts when nothing was expected or found), then any kind of
    /// promise that appeared.
    pub kinds: Vec<ClassifyEvalAskKindData>,
    /// Exact match per field on matched items.
    pub fields: Vec<ClassifyEvalFieldData>,
    /// Matched items with two rows for one expected item.
    pub duplicates: u32,
    /// Expected items absorbed into another email's row of the same sender,
    /// kind and counterparty (two accounts from one sender).
    pub merges: u32,
    pub twins: ClassifyEvalTwinsData,
    pub slices: Vec<ClassifyEvalSliceData>,
    pub reply_owed: ClassifyEvalReplyData,
}
