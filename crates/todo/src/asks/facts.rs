use super::AskInput;
use mxr_core::asks::*;
use mxr_reader::{authored_text, EarlierMessage};
use once_cell::sync::Lazy;
use regex::Regex;

static NEXT_ACTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)\s+(?:and|then|but|however)\s+(?P<next>(?:please\s+)?(?:pay|sign|verify|send|provide|upload|book|contact)\b)"
).expect("action conjunction regex")
});

pub(super) fn read(input: &AskInput<'_>) -> AskFacts {
    let owner_authored = (input.is_owner)(&input.from.email);
    let recipient_role = if input.list_mail {
        RecipientRole::List
    } else if input.to.iter().any(|a| (input.is_owner)(&a.email)) {
        if input.to.iter().all(|a| (input.is_owner)(&a.email)) {
            RecipientRole::AloneTo
        } else {
            RecipientRole::SharedTo
        }
    } else if input.cc.iter().any(|a| (input.is_owner)(&a.email)) {
        RecipientRole::Cc
    } else if input.bcc.iter().any(|a| (input.is_owner)(&a.email)) {
        RecipientRole::Bcc
    } else {
        RecipientRole::Unknown
    };
    let earlier: Vec<_> = input
        .predecessor
        .map(|p| EarlierMessage {
            text: p.text,
            same_author: p.same_author,
        })
        .into_iter()
        .collect();
    let raw = input.body.unwrap_or_default();
    let authored = authored_text(raw, &earlier);
    let mut zones = vec![AskZone {
        kind: ZoneKind::Subject,
        author: Some(input.from.email.clone()),
        text: input.subject.into(),
    }];
    let mut context = None;
    let mut context_author = None;
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("--") && lower.contains("forwarded message")
            || lower == "begin forwarded message:"
        {
            context = Some(ZoneKind::Forwarded);
            context_author = attributed_address(trimmed);
        } else if lower.starts_with("on ") && lower.ends_with("wrote:")
            || lower.starts_with("--") && lower.contains("original message")
            || lower.starts_with("from:")
                && !authored.contains(trimmed)
                && context != Some(ZoneKind::Forwarded)
        {
            context = Some(ZoneKind::Quoted);
            context_author = attributed_address(trimmed);
        } else if context.is_none()
            && (trimmed == "--" || lower == "best regards," || lower == "kind regards,")
        {
            context = Some(ZoneKind::Signature);
            context_author = Some(input.from.email.clone());
        }
        if matches!(context, Some(ZoneKind::Forwarded | ZoneKind::Quoted))
            && lower.starts_with("from:")
        {
            context_author = attributed_address(trimmed);
        }
        let kind = context.unwrap_or_else(|| {
            if trimmed.starts_with('>') {
                ZoneKind::Quoted
            } else if lower.contains("unsubscribe") || lower.contains("confidentiality notice") {
                ZoneKind::Footer
            } else if !trimmed.is_empty() && !authored.contains(trimmed) {
                ZoneKind::Unattributed
            } else if lower.contains("special offer")
                || lower.contains("discount code")
                || lower.contains("limited-time")
            {
                ZoneKind::Advertising
            } else {
                ZoneKind::Authored
            }
        });
        let author = if matches!(kind, ZoneKind::Quoted | ZoneKind::Forwarded) {
            context_author.clone()
        } else {
            Some(input.from.email.clone())
        };
        if let Some(last) = zones
            .last_mut()
            .filter(|z| z.kind == kind && z.author == author)
        {
            last.text.push_str(line);
        } else {
            zones.push(AskZone {
                kind,
                author,
                text: line.into(),
            });
        }
    }
    let shape = zones
        .iter()
        .filter(|z| matches!(z.kind, ZoneKind::Authored | ZoneKind::Subject))
        .map(|z| {
            z.text
                .chars()
                .map(|c| {
                    if c.is_numeric() {
                        '#'
                    } else {
                        c.to_ascii_lowercase()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    AskFacts {
        zones,
        recipient_role,
        owner_authored,
        owner_names: input.owner_names.to_vec(),
        owner_addresses: input.owner_addresses.to_vec(),
        predecessor: input.predecessor.map(|p| p.id.clone()),
        owner_replied_afterwards: input.owner_replied_afterwards,
        amounts: Vec::new(),
        dates: Vec::new(),
        references: Vec::new(),
        template_key: mxr_store::assessment_fingerprint(&shape),
    }
}

fn attributed_address(line: &str) -> Option<String> {
    let address = if let Some((_, tail)) = line.split_once('<') {
        tail.split_once('>')?.0
    } else {
        let (header, value) = line.split_once(':')?;
        if !header.eq_ignore_ascii_case("from") {
            return None;
        }
        value.trim()
    };
    (address.contains('@') && !address.chars().any(char::is_whitespace))
        .then(|| address.to_string())
}

/// Sentence/line units retain byte offsets and never split decimal money.
pub(super) fn unit_spans(text: &str) -> Vec<AskSpan> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        let end = at + ch.len_utf8();
        let continuation = matches!(ch, '\n' | '.' | ';' | '?')
            && billing_continuation(&text[start..at], &text[end..], ch);
        let boundary = ch == '\n' && !continuation
            || ch == ';' && !continuation
            || matches!(ch, '.' | '?' | '!')
                && !continuation
                && text
                    .get(end..)
                    .is_some_and(|s| s.is_empty() || s.starts_with(char::is_whitespace));
        if boundary {
            push_span(text, start, end, &mut spans);
            start = end;
        }
    }
    push_span(text, start, text.len(), &mut spans);
    let mut requests: Vec<AskSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(previous) = requests.last_mut().filter(|previous| {
            !text[previous.end..span.start].contains("\n\n")
                && super::completion::continues(
                    &text[previous.start..previous.end],
                    &text[span.start..span.end],
                )
        }) {
            previous.end = span.end;
        } else {
            requests.push(span);
        }
    }
    let mut clauses = Vec::new();
    for span in requests {
        let mut start = span.start;
        for conjunction in NEXT_ACTION.captures_iter(&text[span.start..span.end]) {
            let (Some(whole), Some(next)) = (conjunction.get(0), conjunction.name("next")) else {
                continue;
            };
            if next.as_str().eq_ignore_ascii_case("sign")
                && text[span.start..span.start + whole.start()]
                    .trim_end()
                    .to_ascii_lowercase()
                    .ends_with("review")
            {
                continue;
            }
            if super::completion::delivery_tail(
                &text[span.start..span.start + whole.start()],
                &text[span.start + next.start()..span.end],
                whole.as_str(),
            ) {
                continue;
            }
            // Signing and returning the same document is one obligation.
            if next.as_str().eq_ignore_ascii_case("send")
                && text[span.start..span.start + whole.start()]
                    .to_ascii_lowercase()
                    .contains("sign")
                && text[span.start + next.start()..]
                    .to_ascii_lowercase()
                    .starts_with("send it back")
            {
                continue;
            }
            push_span(text, start, span.start + whole.start(), &mut clauses);
            start = span.start + next.start();
        }
        push_span(text, start, span.end, &mut clauses);
    }
    clauses
}

fn billing_continuation(before: &str, after: &str, separator: char) -> bool {
    if separator == '\n' && after.starts_with('\n') || after.starts_with("\n\n") {
        return false;
    }
    let before = before.to_ascii_lowercase();
    if before.contains("confirm your delivery address")
        && after
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("reply to this email with")
    {
        return true;
    }
    if after.starts_with("\n\n") {
        return false;
    }
    let after = after
        .trim_start()
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let after = after
        .strip_prefix("please ")
        .filter(|rest| {
            rest.starts_with("update your ")
                && !super::candidates::billing_fields_conflict(&before, rest)
        })
        .unwrap_or(&after);
    [
        "invoice",
        "bill",
        "statement",
        "amount due",
        "minimum payment",
        "payment",
        "card",
        "payout",
        "balance",
        "charge",
    ]
    .iter()
    .any(|word| before.contains(word))
        && [
            "amount due",
            "total due",
            "minimum payment",
            "payment due",
            "due ",
            "due:",
            "account number",
            "account ref",
            "please pay",
            "pay by",
            "please settle",
            "update your card",
            "update your payment",
            "update your bank",
        ]
        .iter()
        .any(|field| after.starts_with(field))
}

fn push_span(text: &str, start: usize, end: usize, spans: &mut Vec<AskSpan>) {
    let slice = &text[start..end];
    let trimmed = slice.trim();
    if !trimmed.is_empty() {
        let left = slice.len() - slice.trim_start().len();
        spans.push(AskSpan {
            start: start + left,
            end: start + left + trimmed.len(),
        });
    }
}
