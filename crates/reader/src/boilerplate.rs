use once_cell::sync::Lazy;
use regex::Regex;

fn compile_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("boilerplate regex literals should compile")
}

static BOILERPLATE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        compile_regex(r"(?i)this (email|message|communication) is confidential"),
        compile_regex(r"(?i)if you (have )?received this (email|message) in error"),
        compile_regex(r"(?i)^DISCLAIMER"),
        compile_regex(
            r"(?i)this (email|message) (and any attachments )?(is|are) intended (only |solely )?for",
        ),
        compile_regex(r"(?i)please consider the environment before printing"),
        compile_regex(r"(?i)any (views|opinions) expressed .* are (solely |those of )"),
        compile_regex(r"(?i)privileged and confidential"),
        compile_regex(r"(?i)if you are not the intended recipient"),
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
