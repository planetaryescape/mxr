use once_cell::sync::Lazy;
use regex::Regex;

static TRACKING_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)view (this )?(email |message )?in (your )?browser").unwrap(),
        Regex::new(r"(?i)update your preferences").unwrap(),
        Regex::new(r"(?i)manage (your )?(email )?preferences").unwrap(),
        Regex::new(r"(?i)you (are )?receiving this (because|email)").unwrap(),
        Regex::new(r"(?i)to stop receiving these").unwrap(),
        Regex::new(r"(?i)click here to unsubscribe").unwrap(),
        Regex::new(r"(?i)no longer wish to receive").unwrap(),
        Regex::new(r"(?i)©\s*\d{4}").unwrap(),
        Regex::new(r"(?i)all rights reserved").unwrap(),
    ]
});

/// Strip tracking/footer junk lines from both ends of message.
pub fn strip(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // Strip from top: contiguous tracking lines at start
    let mut content_start = 0;
    for (i, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let is_tracking = TRACKING_PATTERNS.iter().any(|p| p.is_match(line));
        if is_tracking {
            content_start = i + 1;
        } else {
            break;
        }
    }

    // Strip from bottom: contiguous tracking lines at end
    let mut content_end = lines.len();
    for i in (content_start..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            continue;
        }
        let is_tracking = TRACKING_PATTERNS.iter().any(|p| p.is_match(line));
        if is_tracking {
            content_end = i;
        } else {
            break;
        }
    }

    lines[content_start..content_end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_unsubscribe_footer() {
        let text =
            "Check out our new product!\n\nClick here to unsubscribe\n© 2026 Acme Corp. All rights reserved.";
        let result = strip(text);
        assert!(result.contains("new product"));
        assert!(!result.contains("unsubscribe"));
        assert!(!result.contains("2026"));
    }

    #[test]
    fn strips_view_in_browser() {
        let text =
            "Newsletter content here.\n\nView this email in your browser\nUpdate your preferences";
        let result = strip(text);
        assert!(result.contains("Newsletter content"));
        assert!(!result.contains("View this email"));
    }

    #[test]
    fn preserves_text_without_tracking() {
        let text = "A normal personal email.";
        let result = strip(text);
        assert_eq!(result, text);
    }
}
