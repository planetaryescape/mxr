//! Verified sender origin and receiver handling age (D193).
//! A fresh intact copy can qualify without proving new SMTP delivery. Adapters
//! authenticate original bytes; sync records live eligibility independently.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Verified receiver handling time, origin and stable original identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalEvidence {
    /// Authenticated receiver handling time. Never API internalDate, IMAP
    /// INTERNALDATE or the sender Date. The legacy field name is retained.
    pub received_at: Option<DateTime<Utc>>,
    pub sender: SenderCheck,
    /// Stable authenticated original-message identity across copies/reseals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_fingerprint: Option<String>,
    /// Set by sync, never by mail: fetched after a completed initial sync,
    /// outside a provider history/refetch page, first seen unread in Inbox.
    /// The first stored eligibility wins.
    #[serde(default)]
    pub is_live: bool,
}

impl Default for ArrivalEvidence {
    fn default() -> Self {
        Self {
            received_at: None,
            message_fingerprint: None,
            is_live: false,
            sender: SenderCheck::Unverified {
                reason: UnverifiedReason::NotRecorded,
            },
        }
    }
}

/// Whether the receiving provider vouched for the From host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SenderCheck {
    /// A cryptographically bound receiver verdict says DMARC passed for
    /// `host`, which is the message's From host.
    Verified {
        host: String,
    },
    Unverified {
        reason: UnverifiedReason,
    },
}

/// Why a message was not verified. Stored as `code()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UnverifiedReason {
    /// The message was synced before mxr kept this, or by an adapter that
    /// does not.
    NotRecorded,
    /// The API supplies message headers but no attestation that the receiver
    /// authored a cryptographically bound verdict and handling time.
    GmailVerdictUnproven,
    /// An IMAP server that is not Gmail has no pinned verdict. Outlook.com
    /// and Microsoft 365 are IMAP too, and write no authserv-id, so a header
    /// a relay added cannot be told from the server's own.
    ImapGivesNoVerdict,
    /// The provider's `Authentication-Results` is not there.
    NoVerdictHeader,
    /// Two or more headers carry the provider's id, or one did not parse.
    AmbiguousVerdict,
    /// The provider's verdict is not a DMARC pass for the From host.
    NotDmarcPass,
    /// The provider gave no receipt time, so history cannot be ordered.
    NoReceiptTime,
}

impl UnverifiedReason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotRecorded => "not_recorded",
            Self::GmailVerdictUnproven => "gmail_verdict_unproven",
            Self::ImapGivesNoVerdict => "imap_gives_no_verdict",
            Self::NoVerdictHeader => "no_verdict_header",
            Self::AmbiguousVerdict => "ambiguous_verdict",
            Self::NotDmarcPass => "not_dmarc_pass",
            Self::NoReceiptTime => "no_receipt_time",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [
            Self::NotRecorded,
            Self::GmailVerdictUnproven,
            Self::ImapGivesNoVerdict,
            Self::NoVerdictHeader,
            Self::AmbiguousVerdict,
            Self::NotDmarcPass,
            Self::NoReceiptTime,
        ]
        .into_iter()
        .find(|reason| reason.code() == code)
    }
}
