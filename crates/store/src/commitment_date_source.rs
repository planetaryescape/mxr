//! Where a commitment's date came from, and the one rule for which dates
//! mxr acts on. A date a model wrote before due words existed (D111) stays on
//! the row and is shown as unverified; nothing nags on it.

/// Provenance of `contact_commitments.by_when`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByWhenSource {
    /// Written by a model before R0 resolved dates from the email's words.
    ModelDate,
    /// Resolved by `natural_time` from words that appear in the email.
    Words,
    /// Typed by the user.
    User,
}

impl ByWhenSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ModelDate => "model_date",
            Self::Words => "words",
            Self::User => "user",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "model_date" => Some(Self::ModelDate),
            "words" => Some(Self::Words),
            "user" => Some(Self::User),
            _ => None,
        }
    }

    /// Whether mxr may act on a date from this source: put the commitment on
    /// the desk as due or overdue, or carry the date into a to-do.
    pub fn is_actionable(self) -> bool {
        matches!(self, Self::Words | Self::User)
    }
}

/// Whether a stored date may be acted on. A date with no recorded source is
/// not: it predates provenance.
pub fn is_actionable_date(source: Option<ByWhenSource>) -> bool {
    source.is_some_and(ByWhenSource::is_actionable)
}

/// The same rule as [`ByWhenSource::is_actionable`], as a SQL condition on
/// the `by_when_source` column, for queries that read dates to act on.
pub(crate) const ACTIONABLE_BY_WHEN_SQL: &str = "by_when_source IN ('words', 'user')";

/// Sets the source on rows written before the column existed: a promise the
/// user recorded keeps its `::promise::` id, and every other dated row was
/// written by a model. Run by migration 094 and again on rows a backup copies
/// from an older schema, which leaves the column empty. Rows that already
/// have a source are left alone.
pub(crate) const BACKFILL_BY_WHEN_SOURCE_SQL: &str = "\
UPDATE contact_commitments SET by_when_source = 'user' \
  WHERE by_when_source IS NULL AND instr(id, '::promise::') > 0; \
UPDATE contact_commitments SET by_when_source = 'model_date' \
  WHERE by_when_source IS NULL AND by_when IS NOT NULL;";

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ByWhenSource; 3] = [
        ByWhenSource::ModelDate,
        ByWhenSource::Words,
        ByWhenSource::User,
    ];

    #[test]
    fn only_words_and_user_dates_are_actionable() {
        let actionable: Vec<_> = ALL.iter().filter(|s| s.is_actionable()).collect();
        assert_eq!(actionable, [&ByWhenSource::Words, &ByWhenSource::User]);
        assert!(!is_actionable_date(None));
    }

    #[test]
    fn the_sql_condition_agrees_with_the_rust_rule() {
        // The two spellings of the rule must not drift apart.
        for source in ALL {
            assert_eq!(
                ACTIONABLE_BY_WHEN_SQL.contains(&format!("'{}'", source.as_str())),
                source.is_actionable(),
                "{source:?}"
            );
            assert_eq!(ByWhenSource::parse(source.as_str()), Some(source));
        }
    }
}
