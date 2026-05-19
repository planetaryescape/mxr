//! Locale provider for user-facing strings.
//!
//! Every user-visible string flows through a `Locale`, so the community can
//! add a new language by appending a new `pub const` to [`AVAILABLE_LOCALES`]
//! without touching dispatch logic. The locale-coverage test in this module
//! asserts every field is non-empty across every locale, so missing
//! translations fail CI rather than ship as empty UI.
//!
//! ## Adding a locale
//!
//! 1. Define a new `pub const FR: Locale = …` with all fields populated.
//! 2. Add `&FR` to [`AVAILABLE_LOCALES`].
//! 3. Run `cargo test --package mxr-core i18n` to verify coverage.
//!
//! Locale selection at runtime:
//! - Daemon reads `MXR_LOCALE` env var, falls back to the `locale` config
//!   key, falls back to `"en"`.
//! - Web SPA fetches `GET /api/v1/i18n` once at startup.

use crate::types::CalendarPartstat;

/// One translation. Plain `&'static str` fields keep everything zero-cost and
/// usable from `const` contexts. Add new field groups as new sub-structs (e.g.
/// `FilterStrings`) — never inline group-specific strings here directly.
#[derive(Debug, Clone, Copy)]
pub struct Locale {
    /// IETF language tag (e.g. `"en"`, `"de"`, `"fr-CA"`).
    pub code: &'static str,
    pub invite: InviteStrings,
    pub status: StatusStrings,
}

#[derive(Debug, Clone, Copy)]
pub struct InviteStrings {
    /// Card title rendered in the bordered block, e.g. `"Calendar invite"`.
    pub card_title: &'static str,

    // Subject-line prefixes for outgoing REPLY emails. Convention from
    // RFC-5546 §3.2.3 (English: "Accepted: …", "Declined: …",
    // "Tentative: …"). The trailing separator (": ") is part of the prefix.
    pub subject_prefix_accepted: &'static str,
    pub subject_prefix_declined: &'static str,
    pub subject_prefix_tentative: &'static str,

    // text/plain body templates for outgoing REPLY emails. `{email}` is
    // substituted with the responding attendee's address. Keep the
    // placeholder syntax simple — no Fluent / ICU dependency.
    pub body_template_accepted: &'static str,
    pub body_template_declined: &'static str,
    pub body_template_tentative: &'static str,

    // Action chip labels for the un-responded card.
    pub chip_label_accept: &'static str,
    pub chip_label_tentative: &'static str,
    pub chip_label_decline: &'static str,

    // State row labels for the responded card.
    pub state_label_accepted: &'static str,
    pub state_label_tentative: &'static str,
    pub state_label_declined: &'static str,

    // Hint lines under the action row.
    pub hint_change_response: &'static str,
    pub hint_comment: &'static str,

    // Variant banners.
    pub banner_cancelled: &'static str,
    pub banner_publish: &'static str,
    pub banner_parse_warning: &'static str,
    pub banner_updated: &'static str,
    pub banner_counter: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct StatusStrings {
    /// Shown during the 1s hold window before the REPLY actually sends.
    pub invite_pending_accept: &'static str,
    pub invite_pending_tentative: &'static str,
    pub invite_pending_decline: &'static str,
    /// Shown after `u` cancels the pending send.
    pub invite_cancelled: &'static str,
}

/// A calendar participation status that can be sent back to an organizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendableCalendarPartstat {
    Accepted,
    Tentative,
    Declined,
}

impl SendableCalendarPartstat {
    pub fn as_calendar_partstat(self) -> CalendarPartstat {
        match self {
            Self::Accepted => CalendarPartstat::Accepted,
            Self::Tentative => CalendarPartstat::Tentative,
            Self::Declined => CalendarPartstat::Declined,
        }
    }
}

impl Locale {
    /// Return the subject-line prefix for a sendable outgoing PARTSTAT.
    pub fn invite_subject_prefix_for(&self, partstat: SendableCalendarPartstat) -> &'static str {
        match partstat {
            SendableCalendarPartstat::Accepted => self.invite.subject_prefix_accepted,
            SendableCalendarPartstat::Declined => self.invite.subject_prefix_declined,
            SendableCalendarPartstat::Tentative => self.invite.subject_prefix_tentative,
        }
    }

    /// Substitute `{email}` into the body template for the given PARTSTAT.
    pub fn invite_body_for(&self, partstat: SendableCalendarPartstat, email: &str) -> String {
        let template = match partstat {
            SendableCalendarPartstat::Accepted => self.invite.body_template_accepted,
            SendableCalendarPartstat::Declined => self.invite.body_template_declined,
            SendableCalendarPartstat::Tentative => self.invite.body_template_tentative,
        };
        template.replace("{email}", email)
    }

    /// Return the localized pending-status label for a given outgoing PARTSTAT.
    pub fn invite_status_pending_for(&self, partstat: SendableCalendarPartstat) -> &'static str {
        match partstat {
            SendableCalendarPartstat::Accepted => self.status.invite_pending_accept,
            SendableCalendarPartstat::Declined => self.status.invite_pending_decline,
            SendableCalendarPartstat::Tentative => self.status.invite_pending_tentative,
        }
    }

    /// Return the localized "you accepted/declined/tentatively-accepted" label
    /// for the card state row.
    pub fn invite_state_label_for(&self, partstat: CalendarPartstat) -> Option<&'static str> {
        match partstat {
            CalendarPartstat::Accepted => Some(self.invite.state_label_accepted),
            CalendarPartstat::Declined => Some(self.invite.state_label_declined),
            CalendarPartstat::Tentative => Some(self.invite.state_label_tentative),
            CalendarPartstat::NeedsAction | CalendarPartstat::Delegated => None,
        }
    }
}

pub const EN: Locale = Locale {
    code: "en",
    invite: InviteStrings {
        card_title: "Calendar invite",

        subject_prefix_accepted: "Accepted: ",
        subject_prefix_declined: "Declined: ",
        subject_prefix_tentative: "Tentative: ",

        body_template_accepted: "{email} has accepted this invitation.",
        body_template_declined: "{email} has declined this invitation.",
        body_template_tentative: "{email} has tentatively accepted this invitation.",

        chip_label_accept: "Accept",
        chip_label_tentative: "Maybe",
        chip_label_decline: "Decline",

        state_label_accepted: "\u{2713} You accepted",
        state_label_tentative: "? You said maybe",
        state_label_declined: "\u{2717} You declined",

        hint_change_response: "press ia/im/id to change",
        hint_comment: "Shift+iA/iM/iD to comment",

        banner_cancelled: "Event canceled by organizer",
        banner_publish: "Informational \u{2014} no reply expected",
        banner_parse_warning: "Calendar invite could not be parsed",
        banner_updated: "Updated invite",
        banner_counter: "Counter-proposal received",
    },
    status: StatusStrings {
        invite_pending_accept: "Accepting invite \u{2014} u to undo (1s)",
        invite_pending_tentative: "Tentatively accepting invite \u{2014} u to undo (1s)",
        invite_pending_decline: "Declining invite \u{2014} u to undo (1s)",
        invite_cancelled: "Cancelled \u{2014} no reply sent",
    },
};

/// All shipped locales. To add a translation, append a new `&YourLocale`
/// constant here. See `docs/contributing/localization.md`.
pub const AVAILABLE_LOCALES: &[&Locale] = &[&EN];

/// Default locale (English). Use as a fallback when selection fails.
pub const DEFAULT_LOCALE: &Locale = &EN;

/// Resolve a locale code (e.g. `"en"`, `"de"`, `"fr-CA"`) against the available
/// locales. Falls back to [`DEFAULT_LOCALE`] (`EN`) when no exact match is
/// found. Matching is case-insensitive.
pub fn select(code: &str) -> &'static Locale {
    let needle = code.trim().to_ascii_lowercase();
    for locale in AVAILABLE_LOCALES {
        if locale.code.eq_ignore_ascii_case(&needle) {
            return locale;
        }
    }
    DEFAULT_LOCALE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catch missing translations at CI time. Every shipped locale must
    /// populate every `&'static str` field.
    #[test]
    fn locale_coverage_no_empty_strings() {
        for locale in AVAILABLE_LOCALES {
            let l = **locale;
            assert!(!l.code.is_empty(), "locale code must not be empty");

            // Invite strings
            let i = l.invite;
            for (field_name, value) in [
                ("card_title", i.card_title),
                ("subject_prefix_accepted", i.subject_prefix_accepted),
                ("subject_prefix_declined", i.subject_prefix_declined),
                ("subject_prefix_tentative", i.subject_prefix_tentative),
                ("body_template_accepted", i.body_template_accepted),
                ("body_template_declined", i.body_template_declined),
                ("body_template_tentative", i.body_template_tentative),
                ("chip_label_accept", i.chip_label_accept),
                ("chip_label_tentative", i.chip_label_tentative),
                ("chip_label_decline", i.chip_label_decline),
                ("state_label_accepted", i.state_label_accepted),
                ("state_label_tentative", i.state_label_tentative),
                ("state_label_declined", i.state_label_declined),
                ("hint_change_response", i.hint_change_response),
                ("hint_comment", i.hint_comment),
                ("banner_cancelled", i.banner_cancelled),
                ("banner_publish", i.banner_publish),
                ("banner_parse_warning", i.banner_parse_warning),
                ("banner_updated", i.banner_updated),
                ("banner_counter", i.banner_counter),
            ] {
                assert!(
                    !value.is_empty(),
                    "locale {} field invite.{} is empty",
                    l.code,
                    field_name,
                );
            }

            let s = l.status;
            for (field_name, value) in [
                ("invite_pending_accept", s.invite_pending_accept),
                ("invite_pending_tentative", s.invite_pending_tentative),
                ("invite_pending_decline", s.invite_pending_decline),
                ("invite_cancelled", s.invite_cancelled),
            ] {
                assert!(
                    !value.is_empty(),
                    "locale {} field status.{} is empty",
                    l.code,
                    field_name,
                );
            }
        }
    }

    #[test]
    fn body_templates_contain_email_placeholder() {
        for locale in AVAILABLE_LOCALES {
            for (label, tpl) in [
                ("accepted", locale.invite.body_template_accepted),
                ("declined", locale.invite.body_template_declined),
                ("tentative", locale.invite.body_template_tentative),
            ] {
                assert!(
                    tpl.contains("{email}"),
                    "locale {} body_template_{} missing {{email}} placeholder",
                    locale.code,
                    label,
                );
            }
        }
    }

    #[test]
    fn select_known_locale_returns_it() {
        assert_eq!(select("en").code, "en");
        assert_eq!(select("EN").code, "en");
        assert_eq!(select(" en ").code, "en");
    }

    #[test]
    fn select_unknown_locale_falls_back_to_default() {
        assert_eq!(select("xx-YY").code, DEFAULT_LOCALE.code);
        assert_eq!(select("").code, DEFAULT_LOCALE.code);
    }

    #[test]
    fn subject_prefix_dispatch_matches_partstat() {
        use SendableCalendarPartstat::{Accepted, Declined, Tentative};

        assert_eq!(EN.invite_subject_prefix_for(Accepted), "Accepted: ");
        assert_eq!(EN.invite_subject_prefix_for(Declined), "Declined: ");
        assert_eq!(EN.invite_subject_prefix_for(Tentative), "Tentative: ");
    }

    #[test]
    fn body_dispatch_substitutes_email() {
        let body = EN.invite_body_for(SendableCalendarPartstat::Accepted, "alice@example.com");
        assert_eq!(body, "alice@example.com has accepted this invitation.");
    }

    #[test]
    fn state_label_is_none_for_unresponded() {
        assert!(EN
            .invite_state_label_for(CalendarPartstat::NeedsAction)
            .is_none());
        assert!(EN
            .invite_state_label_for(CalendarPartstat::Delegated)
            .is_none());
        assert!(EN
            .invite_state_label_for(CalendarPartstat::Accepted)
            .is_some());
    }
}
