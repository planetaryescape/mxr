use once_cell::sync::Lazy;
use regex::Regex;

static BOILERPLATE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)this (email|message|communication) is confidential").unwrap(),
        Regex::new(r"(?i)if you (have )?received this (email|message) in error").unwrap(),
        Regex::new(r"(?i)^DISCLAIMER").unwrap(),
        Regex::new(r"(?i)this (email|message) (and any attachments )?(is|are) intended (only |solely )?for")
            .unwrap(),
        Regex::new(r"(?i)please consider the environment before printing").unwrap(),
        Regex::new(r"(?i)any (views|opinions) expressed .* are (solely |those of )").unwrap(),
        Regex::new(r"(?i)privileged and confidential").unwrap(),
        Regex::new(r"(?i)if you are not the intended recipient").unwrap(),
    ]
});

/// Strip legal/confidentiality boilerplate from the end of the message.
pub fn strip(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let mut boilerplate_start = None;
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            continue;
        }
        let is_boilerplate = BOILERPLATE_PATTERNS.iter().any(|p| p.is_match(line));
        if is_boilerplate {
            boilerplate_start = Some(i);
        } else if boilerplate_start.is_some() {
            break;
        }
    }

    match boilerplate_start {
        Some(start) => lines[..start].join("\n"),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_confidentiality_notice() {
        let text = "Hey, let's meet tomorrow.\n\nBest,\nAlice\n\nThis email is confidential and intended solely for the recipient.";
        let result = strip(text);
        assert!(result.contains("meet tomorrow"));
        assert!(!result.contains("confidential"));
    }

    #[test]
    fn preserves_text_without_boilerplate() {
        let text = "Normal message without any boilerplate.";
        let result = strip(text);
        assert_eq!(result, text);
    }
}
