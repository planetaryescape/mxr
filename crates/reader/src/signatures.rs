use once_cell::sync::Lazy;
use regex::Regex;

static SENT_FROM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^sent from (my )?(iphone|ipad|android|galaxy|samsung|outlook|mail)")
        .expect("signature regex literal should compile")
});

static PHONE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[\+]?\d[\d\s\-\(\)]{7,}").expect("phone regex literal should compile")
});

/// Strip the signature from plain text.
/// Returns (cleaned_text, extracted_signature).
pub fn strip(text: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();

    // RFC 3676: "-- \n" delimiter (note trailing space)
    if let Some(pos) = lines.iter().position(|l| *l == "-- " || *l == "--") {
        let body = lines[..pos].join("\n");
        let sig = lines[pos + 1..].join("\n");
        return (body, Some(sig));
    }

    // Heuristic: "Sent from" pattern at end
    for i in (lines.len().saturating_sub(5)..lines.len()).rev() {
        if SENT_FROM.is_match(lines[i].trim()) {
            let body = lines[..i].join("\n");
            let sig = lines[i..].join("\n");
            return (body, Some(sig));
        }
    }

    // Heuristic: block of short lines at end with phone numbers, emails, URLs
    let mut sig_start = None;
    let mut consecutive_short = 0;
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            if consecutive_short >= 3 {
                sig_start = Some(i + 1);
                break;
            }
            consecutive_short = 0;
            continue;
        }
        let is_sig_line = line.len() < 60
            && (PHONE_PATTERN.is_match(line)
                || line.contains('@')
                || line.contains("http")
                || line.contains("www."));
        if is_sig_line {
            consecutive_short += 1;
        } else {
            break;
        }
    }

    if let Some(start) = sig_start {
        let body = lines[..start].join("\n");
        let sig = lines[start..].join("\n");
        return (body, Some(sig));
    }

    (text.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_rfc3676_signature() {
        let text = "Hello world\n\nThanks!\n-- \nJohn Doe\nAcme Corp";
        let (body, sig) = strip(text);
        assert_eq!(body, "Hello world\n\nThanks!");
        assert_eq!(
            sig.expect("signature delimiter should produce a signature"),
            "John Doe\nAcme Corp"
        );
    }

    #[test]
    fn strips_sent_from() {
        let text = "Quick reply.\n\nSent from my iPhone";
        let (body, sig) = strip(text);
        assert_eq!(body.trim(), "Quick reply.");
        assert!(sig
            .expect("sent-from footer should be extracted")
            .contains("Sent from"));
    }

    #[test]
    fn no_signature_returns_original() {
        let text = "Just a normal message with no signature.";
        let (body, sig) = strip(text);
        assert_eq!(body, text);
        assert!(sig.is_none());
    }

    #[test]
    fn strips_double_dash_without_space() {
        let text = "Body here.\n--\nSig line";
        let (body, sig) = strip(text);
        assert_eq!(body, "Body here.");
        assert_eq!(
            sig.expect("double-dash delimiter should produce a signature"),
            "Sig line"
        );
    }
}
