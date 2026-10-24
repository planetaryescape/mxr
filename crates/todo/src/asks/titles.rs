//! Display labels project typed actions; the exact quote stays separate evidence.
use crate::text;
use mxr_core::asks::ActionKind;

pub(super) fn title(action: ActionKind, quote: &str, subject: &str, person: &str) -> String {
    match action {
        ActionKind::Pay => {
            if let Some(thing) =
                crate::detect::bill_thing(subject).or_else(|| crate::detect::bill_thing(quote))
            {
                return format!("Pay {thing}");
            }
            return format!("Pay {person}");
        }
        ActionKind::FixPayment => return format!("Fix payment for {person}"),
        ActionKind::Verify => {
            let cleaned = text::strip_urls(quote);
            let lower = cleaned.to_ascii_lowercase();
            for verb in ["verify ", "confirm ", "activate "] {
                if let Some(start) = lower.find(verb) {
                    let words = cleaned[start..].split(':').next().unwrap_or_default();
                    return text::clip(
                        &format!(
                            "{} for {person}",
                            text::capitalise(words.trim_end_matches(['.', '!', '?']))
                        ),
                        120,
                    );
                }
            }
        }
        _ => {}
    }
    let cleaned = text::strip_urls(quote);
    let mut words = cleaned.trim().trim_end_matches(['.', '!', '?']);
    for prefix in [
        "I'll ", "I will ", "We'll ", "We will ", "Please ", "please ",
    ] {
        if let Some(rest) = words.strip_prefix(prefix) {
            words = rest;
            break;
        }
    }
    let words = words
        .strip_prefix("send you ")
        .map_or_else(|| words.to_string(), |rest| format!("send {rest}"));
    text::capitalise(&text::clip(&words, 120))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verification_labels_omit_links_without_editing_evidence() {
        let quote = "Please verify your email address at https://example.org/verify";
        let shown = title(ActionKind::Verify, quote, "", "Acme");
        assert_eq!(shown, "Verify your email address for Acme");
        assert_eq!(
            quote,
            "Please verify your email address at https://example.org/verify"
        );
    }

    #[test]
    fn verification_labels_bound_long_text_without_editing_evidence() {
        let long = format!(
            "Please verify your email address {}",
            "details ".repeat(100)
        );
        assert!(title(ActionKind::Verify, &long, "", "Acme").chars().count() <= 120);
        assert!(long.len() > 700, "source evidence remains complete");
    }

    #[test]
    fn task_labels_are_readable_without_rewriting_source_quotes() {
        assert_eq!(
            title(
                ActionKind::Send,
                "I'll send you the signed form.",
                "",
                "Priya"
            ),
            "Send the signed form"
        );
        assert_eq!(
            title(
                ActionKind::Other,
                "I'll go through the tutorial at https://example.org/manual",
                "",
                "Priya"
            ),
            "Go through the tutorial"
        );
        assert_eq!(
            title(
                ActionKind::Pay,
                "Amount due £20",
                "Council tax bill",
                "Council"
            ),
            "Pay council tax"
        );
        assert_eq!(
            title(
                ActionKind::Verify,
                "Please verify your email address: https://example.org",
                "",
                "Acme"
            ),
            "Verify your email address for Acme"
        );
    }
}
