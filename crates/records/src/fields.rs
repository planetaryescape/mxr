//! A record's fields, their values and where each came from.

use crate::Source;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use mxr_core::id::MessageId;
use mxr_store::RecordFieldValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldName {
    Kind,
    Issuer,
    Title,
    Reference,
    Amount,
    IssuedAt,
    SpanStart,
    SpanEnd,
    Place,
    DeliveredAt,
    ReturnBy,
    WarrantyUntil,
    ValidUntil,
}

impl FieldName {
    pub const ALL: [Self; 13] = [
        Self::Kind,
        Self::Issuer,
        Self::Title,
        Self::Reference,
        Self::Amount,
        Self::IssuedAt,
        Self::SpanStart,
        Self::SpanEnd,
        Self::Place,
        Self::DeliveredAt,
        Self::ReturnBy,
        Self::WarrantyUntil,
        Self::ValidUntil,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kind => "kind",
            Self::Issuer => "issuer",
            Self::Title => "title",
            Self::Reference => "reference",
            Self::Amount => "amount",
            Self::IssuedAt => "issued_at",
            Self::SpanStart => "span_start",
            Self::SpanEnd => "span_end",
            Self::Place => "place",
            Self::DeliveredAt => "delivered_at",
            Self::ReturnBy => "return_by",
            Self::WarrantyUntil => "warranty_until",
            Self::ValidUntil => "valid_until",
        }
    }

    /// Accepts the stored name and a few spoken ones ("date", "ref",
    /// "total").
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Some(match value.as_str() {
            "kind" | "type" => Self::Kind,
            "issuer" | "merchant" | "from" => Self::Issuer,
            "title" | "what" => Self::Title,
            "reference" | "ref" | "order" | "number" => Self::Reference,
            "amount" | "total" | "price" => Self::Amount,
            "issued_at" | "issued" | "date" | "issued_on" => Self::IssuedAt,
            "span_start" | "start" | "starts" => Self::SpanStart,
            "span_end" | "end" | "ends" => Self::SpanEnd,
            "place" | "where" | "destination" => Self::Place,
            "delivered_at" | "delivered" => Self::DeliveredAt,
            "return_by" | "return" => Self::ReturnBy,
            "warranty_until" | "warranty" => Self::WarrantyUntil,
            "valid_until" | "valid" | "expires" => Self::ValidUntil,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Kind => "Kind",
            Self::Issuer => "From",
            Self::Title => "What",
            Self::Reference => "Reference",
            Self::Amount => "Amount",
            Self::IssuedAt => "Date",
            Self::SpanStart => "Starts",
            Self::SpanEnd => "Ends",
            Self::Place => "Where",
            Self::DeliveredAt => "Delivered",
            Self::ReturnBy => "Return by",
            Self::WarrantyUntil => "Warranty to",
            Self::ValidUntil => "Valid until",
        }
    }

    /// Money and dates: the fields a record must have checked to be
    /// checked.
    pub fn needs_checking(self) -> bool {
        matches!(
            self,
            Self::Amount
                | Self::IssuedAt
                | Self::SpanStart
                | Self::SpanEnd
                | Self::DeliveredAt
                | Self::ReturnBy
                | Self::WarrantyUntil
                | Self::ValidUntil
        )
    }

    pub fn is_date(self) -> bool {
        self.needs_checking() && self != Self::Amount
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Money { minor: i64, currency: String },
    At(DateTime<Utc>),
}

/// One field read from one email, with its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub field: FieldName,
    pub value: FieldValue,
    pub source: Source,
    /// False for a value the user should confirm: anything a rule read for
    /// money or a date.
    pub checked: bool,
    /// The words it was read from, verbatim, or how it was worked out.
    pub evidence: Option<String>,
}

impl Found {
    pub fn text(field: FieldName, source: Source, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            field,
            evidence: Some(value.clone()),
            value: FieldValue::Text(value),
            source,
            checked: true,
        }
    }

    pub fn at(
        field: FieldName,
        source: Source,
        at: DateTime<Utc>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            field,
            value: FieldValue::At(at),
            checked: source_checks_money_and_dates(source),
            source,
            evidence: Some(evidence.into()),
        }
    }

    pub fn money(source: Source, minor: i64, currency: &str, evidence: impl Into<String>) -> Self {
        Self {
            field: FieldName::Amount,
            value: FieldValue::Money {
                minor,
                currency: currency.to_ascii_uppercase(),
            },
            checked: source_checks_money_and_dates(source),
            source,
            evidence: Some(evidence.into()),
        }
    }

    #[must_use]
    pub fn unchecked(mut self) -> Self {
        self.checked = false;
        self
    }

    /// The store's row for this value, from `source_key` (the message id,
    /// `user` or `todo:<id>`).
    pub fn to_store(
        &self,
        source_key: &str,
        message_id: Option<&MessageId>,
        observed_at: DateTime<Utc>,
    ) -> RecordFieldValue {
        let (value_text, value_int) = match &self.value {
            FieldValue::Text(text) => (Some(text.clone()), None),
            FieldValue::Money { minor, currency } => (Some(currency.clone()), Some(*minor)),
            FieldValue::At(at) => (None, Some(at.timestamp())),
        };
        RecordFieldValue {
            field: self.field.as_str().to_string(),
            source_key: source_key.to_string(),
            message_id: message_id.cloned(),
            source: self.source.as_str().to_string(),
            rank: self.source.rank(),
            value_text,
            value_int,
            checked: self.checked || !self.field.needs_checking(),
            evidence: self.evidence.clone(),
            observed_at,
        }
    }
}

/// Markup and you are trusted for money and dates; a rule, a to-do's copy
/// or a tracker's guess is not until confirmed.
fn source_checks_money_and_dates(source: Source) -> bool {
    matches!(source, Source::Schema | Source::User)
}

/// A calendar day stored at midday UTC, so it reads as the same day in
/// every zone from UTC-11 to UTC+11.
pub fn day_at(day: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&day.and_hms_opt(12, 0, 0).unwrap_or_default())
}

/// Whether `evidence` appears in `haystack`, ignoring runs of whitespace
/// and case. Every value a rule shows must be quotable from its email.
pub fn verbatim(haystack: &str, evidence: &str) -> bool {
    let squash = |text: &str| -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let needle = squash(evidence);
    !needle.is_empty() && squash(haystack).contains(&needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_never_check_money_or_dates_but_schema_does() {
        let rule = Found::money(Source::Rule, 100, "gbp", "£1.00");
        assert!(!rule.checked);
        let schema = Found::money(Source::Schema, 100, "GBP", "1.00");
        assert!(schema.checked);
        // Text fields are never what makes a record unchecked.
        let title = Found::text(FieldName::Title, Source::Rule, "XPS 14").to_store(
            "m",
            None,
            day_at(NaiveDate::from_ymd_opt(2025, 3, 3).expect("day")),
        );
        assert!(title.checked);
    }

    #[test]
    fn verbatim_ignores_spacing_and_case() {
        assert!(verbatim(
            "Order total:\n  £1,249.00",
            "order total: £1,249.00"
        ));
        assert!(!verbatim("Order total £1,249.00", "£1,294.00"));
        assert!(!verbatim("anything", "  "));
    }

    #[test]
    fn spoken_field_names_resolve() {
        for (said, field) in [
            ("ref", FieldName::Reference),
            ("date", FieldName::IssuedAt),
            ("total", FieldName::Amount),
            ("return-by", FieldName::ReturnBy),
        ] {
            assert_eq!(FieldName::parse(said), Some(field), "{said}");
        }
    }
}
