//! What the receiving provider itself said about a message at the moment it
//! arrived: when it received it, and whether it vouches for the sender.
//!
//! The provider adapter writes this once, with the message. Nothing in the
//! message's own text (its `Date`, display name or subject) feeds it, and
//! nothing recomputes it later, so classification order, a rebuild or a
//! re-sync cannot change what was true on arrival (blueprint 22, D193).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The provider's receipt time and its verdict on the sender.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalEvidence {
    /// When the provider received the message: Gmail's `internalDate`, IMAP's
    /// `INTERNALDATE`. Never the sender's `Date` header.
    pub received_at: Option<DateTime<Utc>>,
    pub sender: SenderCheck,
    /// Set by sync, never by mail: fetched after a completed initial sync,
    /// outside a provider history/refetch page. The first stored value wins.
    #[serde(default)]
    pub is_live: bool,
}

impl Default for ArrivalEvidence {
    fn default() -> Self {
        Self {
            received_at: None,
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
    /// The provider's own `Authentication-Results` says DMARC passed for
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
    /// authored the verdict or that the message arrived through SMTP.
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
