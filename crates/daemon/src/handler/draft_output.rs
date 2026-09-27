//! Deterministic clean-up of a model's draft before the user sees it.
//!
//! Models wrap an email in chatter ("Here's a draft:"), fences, a
//! `Subject:` line or a `[Your Name]` placeholder even when told not to.
//! None of that should reach a compose window.

/// The email body with model chatter removed. `my_name` fills a leftover
/// name placeholder; anything the model marked `[[?: …]]` stays for the
/// user to fill.
pub(crate) fn clean_draft(raw: &str, my_name: Option<&str>) -> String {
    let mut text = raw.trim().replace("\r\n", "\n");

    // A fenced block is the email; keep its inside.
    if let Some(inner) = fenced(&text) {
        text = inner;
    }

    let mut lines: Vec<&str> = text.lines().collect();
    // Leading chatter: "Here's a draft:", "Sure! Here is your reply:".
    while let Some(first) = lines.first() {
        let lower = first.trim().to_lowercase();
        let chatter = lower.ends_with(':')
            && [
                "here",
                "sure",
                "certainly",
                "below",
                "okay",
                "ok,",
                "draft",
                "reply",
                "of course",
            ]
            .iter()
            .any(|lead| lower.starts_with(lead));
        if chatter || lower.starts_with("subject:") || lower == "---" || lower.is_empty() {
            lines.remove(0);
        } else {
            break;
        }
    }
    // Trailing chatter: "---", "Let me know if you'd like changes", notes.
    while let Some(last) = lines.last() {
        let lower = last.trim().to_lowercase();
        let chatter = lower == "---"
            || lower.is_empty()
            || lower.starts_with("note:")
            || lower.starts_with("(note")
            || lower.starts_with("let me know if you")
            || lower.starts_with("feel free to adjust")
            || lower.starts_with("i hope this draft");
        if chatter {
            lines.pop();
        } else {
            break;
        }
    }
    let mut text = lines.join("\n");

    // A quoted whole: "…".
    if text.len() > 2
        && text.starts_with('"')
        && text.ends_with('"')
        && !text[1..text.len() - 1].contains('"')
    {
        text = text[1..text.len() - 1].to_string();
    }

    for placeholder in [
        "[Your Name]",
        "[Your name]",
        "[your name]",
        "[Name]",
        "[My Name]",
        "<Your Name>",
    ] {
        text = match my_name {
            Some(name) => text.replace(placeholder, name),
            None => text.replace(placeholder, "[[?: your name]]"),
        };
    }
    text.trim().to_string()
}

fn fenced(text: &str) -> Option<String> {
    let start = text.find("```")?;
    let after = &text[start + 3..];
    let body_start = after.find('\n')? + 1;
    let end = after[body_start..].find("```")?;
    Some(after[body_start..body_start + end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_preamble_subject_and_sign_off_chatter() {
        let raw = "Here's a draft of your reply:\n\nSubject: Re: Pricing\n\nHi Alice,\n\nFriday works.\n\nCheers,\n[Your Name]\n\nLet me know if you'd like any changes!";
        assert_eq!(
            clean_draft(raw, Some("Sam")),
            "Hi Alice,\n\nFriday works.\n\nCheers,\nSam"
        );
    }

    #[test]
    fn keeps_the_inside_of_a_fence() {
        let raw = "Sure:\n```\nyep, 2pm works\ns\n```";
        assert_eq!(clean_draft(raw, None), "yep, 2pm works\ns");
    }

    #[test]
    fn leaves_a_clean_draft_and_its_questions_alone() {
        let raw = "Works for me. The budget is [[?: final budget]].";
        assert_eq!(clean_draft(raw, None), raw);
    }

    #[test]
    fn unknown_name_becomes_a_question() {
        assert_eq!(
            clean_draft("Thanks,\n[Your Name]", None),
            "Thanks,\n[[?: your name]]"
        );
    }
}
