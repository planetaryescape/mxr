//! The colour and tag that mark an account's mail (blueprint 33, D147).
//!
//! The daemon owns both values. Clients get the slot name, never a hex value,
//! and map it to their own theme.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// One of eight named account colour slots, in assignment order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AccountColor {
    Sky,
    Amber,
    Green,
    Yellow,
    Blue,
    Vermilion,
    Purple,
    Slate,
}

impl AccountColor {
    /// Every slot in assignment order. A new account takes the first one no
    /// other account holds.
    pub const ALL: [AccountColor; 8] = [
        AccountColor::Sky,
        AccountColor::Amber,
        AccountColor::Green,
        AccountColor::Yellow,
        AccountColor::Blue,
        AccountColor::Vermilion,
        AccountColor::Purple,
        AccountColor::Slate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AccountColor::Sky => "sky",
            AccountColor::Amber => "amber",
            AccountColor::Green => "green",
            AccountColor::Yellow => "yellow",
            AccountColor::Blue => "blue",
            AccountColor::Vermilion => "vermilion",
            AccountColor::Purple => "purple",
            AccountColor::Slate => "slate",
        }
    }
}

impl fmt::Display for AccountColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown account colour '{input}'; use one of {}", allowed_colours())]
pub struct AccountColorError {
    pub input: String,
}

fn allowed_colours() -> String {
    AccountColor::ALL
        .iter()
        .map(|color| color.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

impl FromStr for AccountColor {
    type Err = AccountColorError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        AccountColor::ALL
            .iter()
            .copied()
            .find(|color| color.as_str() == input)
            .ok_or_else(|| AccountColorError {
                input: input.to_string(),
            })
    }
}

/// The colour for the next account. It is the first slot no account holds.
/// When all eight are taken, slots repeat in order, so the ninth account
/// takes the first slot again. `existing_accounts` is how many accounts
/// already exist, not counting the new one.
pub fn next_account_color(taken: &[AccountColor], existing_accounts: usize) -> AccountColor {
    AccountColor::ALL
        .iter()
        .copied()
        .find(|color| !taken.contains(color))
        .unwrap_or(AccountColor::ALL[existing_accounts % AccountColor::ALL.len()])
}

/// Longest tag that fits the account chip.
pub const MAX_ACCOUNT_TAG_LEN: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("account tag '{input}' must be 1 to 3 letters or digits (A-Z, 0-9)")]
pub struct AccountTagError {
    pub input: String,
}

/// Validates a tag and returns it uppercased. Lowercase input is accepted.
pub fn parse_account_tag(input: &str) -> Result<String, AccountTagError> {
    let valid_length = (1..=MAX_ACCOUNT_TAG_LEN).contains(&input.len());
    if valid_length && input.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(input.to_ascii_uppercase())
    } else {
        Err(AccountTagError {
            input: input.to_string(),
        })
    }
}

/// No tag of one to three letters or digits is free. Not reachable in
/// practice, since the search tries the name first, but the bound is real.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no free account tag: every tag of one to three letters or digits is taken")]
pub struct NoFreeAccountTag;

const TAG_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Derives the tag an account is stored with, avoiding `taken`.
///
/// Tries the initials of the first two words ("Planetary Escape" gives PE),
/// then the first two letters of the name, then three, then appends a digit
/// from 2 up to 99 to the initials. Then it tries the first three hex
/// characters of the account id, and finally every tag of one to three
/// characters, shortest first. The result is always a valid tag.
pub fn derive_account_tag(
    name: &str,
    taken: &std::collections::HashSet<String>,
    account_id: &str,
) -> Result<String, NoFreeAccountTag> {
    let letters: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let initials: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_ascii_alphanumeric()))
        .take(2)
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if !letters.is_empty() {
        let candidates = [
            initials.clone(),
            letters.chars().take(2).collect(),
            letters.chars().take(3).collect(),
        ];
        for candidate in candidates.into_iter().filter(|c| !c.is_empty()) {
            if !taken.contains(&candidate) {
                return Ok(candidate);
            }
        }
        let base = if initials.is_empty() {
            letters.as_str()
        } else {
            initials.as_str()
        };
        for suffix in 2..100 {
            let digits = suffix.to_string();
            let stem: String = base.chars().take(3 - digits.len()).collect();
            let candidate = format!("{stem}{digits}");
            if !taken.contains(&candidate) {
                return Ok(candidate);
            }
        }
    }

    let hex: String = account_id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_uppercase())
        .take(3)
        .collect();
    if !hex.is_empty() && !taken.contains(&hex) {
        return Ok(hex);
    }

    for length in 1..=MAX_ACCOUNT_TAG_LEN {
        let total = TAG_ALPHABET.len().pow(length as u32);
        for index in 0..total {
            let candidate = tag_from_index(index, length);
            if !taken.contains(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(NoFreeAccountTag)
}

/// The `index`-th tag of `length` characters, counting in the tag alphabet.
fn tag_from_index(mut index: usize, length: usize) -> String {
    let mut chars = vec![b'0'; length];
    for slot in chars.iter_mut().rev() {
        *slot = TAG_ALPHABET[index % TAG_ALPHABET.len()];
        index /= TAG_ALPHABET.len();
    }
    String::from_utf8(chars).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;

    #[test]
    fn colour_parses_every_slot_name() {
        for color in AccountColor::ALL {
            assert_eq!(color.as_str().parse::<AccountColor>().unwrap(), color);
        }
    }

    #[test]
    fn unknown_colour_error_lists_allowed_names() {
        let error = "teal".parse::<AccountColor>().unwrap_err();
        let message = error.to_string();
        assert!(message.contains("'teal'"), "{message}");
        assert!(message.contains("sky, amber, green"), "{message}");
        assert!(message.contains("slate"), "{message}");
    }

    #[test]
    fn colour_parse_is_case_sensitive_because_config_is_lowercase() {
        assert!("Sky".parse::<AccountColor>().is_err());
    }

    #[test]
    fn first_free_slot_is_taken_for_a_new_account() {
        let taken = [AccountColor::Sky, AccountColor::Green];
        assert_eq!(next_account_color(&taken, 2), AccountColor::Amber);
    }

    #[test]
    fn empty_store_starts_at_sky() {
        assert_eq!(next_account_color(&[], 0), AccountColor::Sky);
    }

    #[test]
    fn ninth_account_repeats_slots_in_order() {
        let taken = AccountColor::ALL;
        assert_eq!(next_account_color(&taken, 8), AccountColor::Sky);
        assert_eq!(next_account_color(&taken, 9), AccountColor::Amber);
        assert_eq!(next_account_color(&taken, 15), AccountColor::Slate);
    }

    #[test]
    fn tag_accepts_one_to_three_alphanumerics_and_uppercases_them() {
        assert_eq!(parse_account_tag("w").unwrap(), "W");
        assert_eq!(parse_account_tag("pe").unwrap(), "PE");
        assert_eq!(parse_account_tag("W2D").unwrap(), "W2D");
    }

    #[test]
    fn tag_rejects_empty_long_and_non_alphanumeric_input() {
        for bad in ["", "WORK", "W-", "W ", "é", "A_B"] {
            assert!(parse_account_tag(bad).is_err(), "{bad:?} should be refused");
        }
    }

    fn taken_of(tags: &[&str]) -> std::collections::HashSet<String> {
        tags.iter().map(|tag| tag.to_string()).collect()
    }

    #[test]
    fn derived_tag_uses_initials_then_letters_then_digits() {
        let id = crate::AccountId::new();
        assert_eq!(
            derive_account_tag("Planetary Escape", &taken_of(&[]), &id.as_str()).unwrap(),
            "PE"
        );
        assert_eq!(
            derive_account_tag("Work", &taken_of(&["W"]), &id.as_str()).unwrap(),
            "WO"
        );
        assert_eq!(
            derive_account_tag("Work", &taken_of(&["W", "WO"]), &id.as_str()).unwrap(),
            "WOR"
        );
        assert_eq!(
            derive_account_tag("Work", &taken_of(&["W", "WO", "WOR"]), &id.as_str()).unwrap(),
            "W2"
        );
    }

    #[test]
    fn derived_tag_never_leaves_the_three_character_bound() {
        let id = crate::AccountId::new();
        let mut taken = taken_of(&["W", "WO", "WOR"]);
        taken.extend((2..100).map(|n| format!("W{n}")));
        let tag = derive_account_tag("Work", &taken, &id.as_str()).unwrap();
        assert!(tag.len() <= MAX_ACCOUNT_TAG_LEN, "{tag}");
        assert!(!taken.contains(&tag), "{tag}");
        assert!(parse_account_tag(&tag).is_ok(), "{tag}");
    }

    #[test]
    fn exhausted_search_is_an_error_not_a_longer_tag() {
        let mut taken = std::collections::HashSet::new();
        for length in 1..=MAX_ACCOUNT_TAG_LEN {
            for index in 0..TAG_ALPHABET.len().pow(length as u32) {
                taken.insert(tag_from_index(index, length));
            }
        }
        let id = crate::AccountId::new();
        assert_eq!(
            derive_account_tag("Work", &taken, &id.as_str()),
            Err(NoFreeAccountTag)
        );
    }

    #[test]
    fn exhaustive_search_returns_short_tags_first() {
        let id = crate::AccountId::new();
        let hex: String = id
            .as_str()
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_uppercase())
            .take(3)
            .collect();
        // Every one-character tag and the id's hex tag are taken, so the search
        // has to move on to two characters, and "00" is the first free one.
        let mut taken = taken_of(&[hex.as_str()]);
        taken.extend((0..TAG_ALPHABET.len()).map(|i| tag_from_index(i, 1)));
        assert_eq!(derive_account_tag("***", &taken, &id.as_str()).unwrap(), "00");
    }
}
