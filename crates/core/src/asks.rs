//! The asks facet covers received and sent mail without assigning a home.
use crate::{AccountId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const ASKS_POLICY_VERSION: &str = "asks-r2c-2";

macro_rules! tags {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
    };
}

tags!(ZoneKind {
    Subject,
    Authored,
    Quoted,
    Forwarded,
    Signature,
    Footer,
    Advertising,
    Unattributed
});
tags!(RecipientRole {
    AloneTo,
    SharedTo,
    Cc,
    Bcc,
    List,
    Unknown
});
tags!(Act {
    Request,
    Commitment,
    Obligation,
    Offer,
    Question,
    Information,
    Pleasantry
});
tags!(Actor {
    Owner,
    Correspondent,
    ThirdParty,
    Everyone,
    Nobody
});
tags!(AskState {
    Pending,
    Done,
    Failed,
    Cancelled,
    Conditional,
    Hypothetical
});
tags!(ActionKind {
    Pay,
    FixPayment,
    Sign,
    Verify,
    Reply,
    Send,
    Provide,
    Upload,
    Book,
    Attend,
    RenewOrCancel,
    Register,
    Contact,
    Other
});
tags!(AskAnswer {
    AsksYou,
    AsksYouToReply,
    YouPromised,
    YouAskedThem,
    TheyPromisedYou,
    Nothing,
    NotSure
});
tags!(DocumentKind {
    Bill,
    Statement,
    PaymentConfirmation,
    AutomaticPayment,
    FailedPayment,
    Credit,
    Offer,
    Other
});
tags!(PaymentStatus {
    Due,
    PastDue,
    Complete,
    Automatic,
    Failed,
    Credit,
    None
});

/// UTF-8 byte offsets into the unmodified zone text; end is exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskZone {
    pub kind: ZoneKind,
    /// An address from the envelope or an explicit attribution; absent if unknown.
    pub author: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskAmount {
    pub zone: usize,
    pub span: AskSpan,
    pub minor: i64,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskDate {
    pub zone: usize,
    pub span: AskSpan,
    pub at: DateTime<Utc>,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskReference {
    pub zone: usize,
    pub span: AskSpan,
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskFacts {
    pub zones: Vec<AskZone>,
    pub recipient_role: RecipientRole,
    pub owner_authored: bool,
    pub owner_names: Vec<String>,
    /// Canonical OwnedAddresses order: primary first, then send-as aliases.
    pub owner_addresses: Vec<String>,
    pub predecessor: Option<MessageId>,
    pub owner_replied_afterwards: bool,
    pub amounts: Vec<AskAmount>,
    pub dates: Vec<AskDate>,
    pub references: Vec<AskReference>,
    /// Local digest of structural text, never a remote template identifier.
    pub template_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskTags {
    pub act: Act,
    pub actor: Actor,
    pub state: AskState,
    pub action: ActionKind,
    pub amount: Option<usize>,
    pub due: Option<usize>,
    pub reference: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskVote {
    pub rule: String,
    pub tags: AskTags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DocumentReading {
    pub kind: DocumentKind,
    pub status: PaymentStatus,
    pub required_fields_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskUnit {
    pub zone: usize,
    pub span: AskSpan,
    pub votes: Vec<AskVote>,
    /// Disagreement or no supported reading is an abstention, never a task.
    pub tags: Option<AskTags>,
    pub document: DocumentReading,
    pub answer: AskAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskAssessment {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub policy_version: String,
    pub input_fingerprint: String,
    pub pending_body: bool,
    /// Local timezone rules and configuration used to interpret date candidates.
    #[serde(default)]
    pub context_token: String,
    pub facts: AskFacts,
    pub units: Vec<AskUnit>,
}

impl AskAssessment {
    /// The exact authored unit is immutable evidence, separate from edited titles.
    pub fn quote(&self, unit: &AskUnit) -> Option<&str> {
        self.facts
            .zones
            .get(unit.zone)?
            .text
            .get(unit.span.start..unit.span.end)
    }
}

/// Bounded mode projection of the persisted answer. Full zones stay in the
/// local assessment; the message, zone and byte span identify the evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskSummary {
    pub message_id: MessageId,
    pub policy_version: String,
    pub input_fingerprint: String,
    pub pending_body: bool,
    pub units: Vec<AskUnitSummary>,
    pub more_units: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AskUnitSummary {
    pub unit: usize,
    pub zone: usize,
    pub span: AskSpan,
    pub author: Option<String>,
    /// Exact source text, or absent when too long for a mode response.
    pub quote: Option<String>,
    pub answer: AskAnswer,
    pub tags: Option<AskTags>,
    pub amount: Option<AskAmount>,
    pub due: Option<AskDate>,
    pub reference: Option<AskReference>,
}

impl AskAssessment {
    pub fn summary(&self) -> AskSummary {
        let mut relevant = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.answer != AskAnswer::Nothing);
        let units = relevant
            .by_ref()
            .take(8)
            .map(|(index, unit)| AskUnitSummary {
                unit: index,
                zone: unit.zone,
                span: unit.span.clone(),
                author: self
                    .facts
                    .zones
                    .get(unit.zone)
                    .and_then(|z| z.author.clone())
                    .filter(|a| a.len() <= 320),
                quote: self
                    .quote(unit)
                    .filter(|q| q.len() <= 2048)
                    .map(str::to_string),
                answer: unit.answer,
                tags: unit.tags.clone(),
                amount: unit
                    .tags
                    .as_ref()
                    .and_then(|t| t.amount)
                    .and_then(|i| self.facts.amounts.get(i))
                    .cloned(),
                due: unit
                    .tags
                    .as_ref()
                    .and_then(|t| t.due)
                    .and_then(|i| self.facts.dates.get(i))
                    .cloned(),
                reference: unit
                    .tags
                    .as_ref()
                    .and_then(|t| t.reference)
                    .and_then(|i| self.facts.references.get(i))
                    .filter(|r| r.value.len() <= 256)
                    .cloned(),
            })
            .collect();
        AskSummary {
            message_id: self.message_id.clone(),
            policy_version: self.policy_version.clone(),
            input_fingerprint: self.input_fingerprint.clone(),
            pending_body: self.pending_body,
            units,
            more_units: relevant.next().is_some(),
        }
    }
}
