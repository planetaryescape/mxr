//! An explicit completion and its anaphoric proof delivery are one request.
use once_cell::sync::Lazy;
use regex::Regex;

static REQUIRED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)\b(?:the|your|this|our|my)\s+(?P<object>[\p{L}\p{N}][\p{L}\p{N} '’-]{0,100}?)\s+(?:must|needs to|is required to)\s+be\s+(?P<verb>completed|finished|submitted)\b"
).expect("required completion")
});
static FOLLOWUP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:please\s+)?(?:finish|complete|submit)\s+(?:it|this|that)\b")
        .expect("completion continuation")
});
static DELIVERY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)^(?:send|provide|upload|return)\s+(?:(?:me|us)\s+)?(?:the|a|your|its)\s+(?:certificate|confirmation|proof|copy|results?)\b"
).expect("completion evidence")
});
static LINK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\s+(?:and|then)\s+").expect("completion link"));

pub(super) fn required(text: &str) -> bool {
    REQUIRED.is_match(text)
}

pub(super) fn continues(before: &str, after: &str) -> bool {
    required(before) && FOLLOWUP.is_match(after.trim_start()) && !super::candidates::has_due(after)
}

pub(super) fn delivery_tail(before: &str, after: &str, link: &str) -> bool {
    let link = link.trim_start().to_ascii_lowercase();
    (link.starts_with("and ") || link.starts_with("then "))
        && required(before)
        && FOLLOWUP.is_match(
            before
                .rsplit(['.', '?', '!', ';', '\n'])
                .next()
                .unwrap_or_default()
                .trim(),
        )
        && DELIVERY.is_match(after.trim_start())
        && !super::candidates::has_due(after)
}

pub(super) fn includes_delivery(text: &str) -> bool {
    LINK.find_iter(text)
        .any(|link| delivery_tail(&text[..link.start()], &text[link.end()..], link.as_str()))
}

pub(super) fn title(text: &str) -> Option<String> {
    let captures = REQUIRED.captures(text)?;
    let verb = match captures
        .name("verb")?
        .as_str()
        .to_ascii_lowercase()
        .as_str()
    {
        "submitted" => "Submit",
        "finished" => "Finish",
        _ => "Complete",
    };
    Some(format!("{verb} {}", captures.name("object")?.as_str()))
}
