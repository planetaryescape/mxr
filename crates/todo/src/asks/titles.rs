//! Display labels project typed actions; the exact quote stays separate evidence.
use crate::text;
use mxr_core::asks::ActionKind;

pub(super) fn title(action: ActionKind, quote: &str, subject: &str, person: &str) -> String {
    if action == ActionKind::Other {
        if let Some(label) = super::completion::title(quote) {
            return text::clip(&label, 120);
        }
    }
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
    fn verification_labels_bound_long_text_without_editing_evidence() {
        let long = format!(
            "Please verify your email address {}",
            "details ".repeat(100)
        );
        assert!(title(ActionKind::Verify, &long, "", "Acme").chars().count() <= 120);
        assert!(long.len() > 700, "source evidence remains complete");
    }
}
