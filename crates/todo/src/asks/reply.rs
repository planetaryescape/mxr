//! Reply wording distinguishes a speaker's promise or offer from a recipient's turn.
use mxr_core::asks::{Act, Actor};
use once_cell::sync::Lazy;
use regex::Regex;

static SUBJECT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(i|we|he|she|they)\b(.*)$").expect("response subject regex"));
static ADDRESSED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\byou\b").expect("response addressee regex"));
static NOMINAL_REQUEST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s+(?:(?:need|require|want|request)\s+|(?:would\s+)?appreciate\s+)(?:a|an|the|your)\s+(?:\p{L}+\s+)*$")
        .expect("nominal response request regex")
});
static COMMITMENT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:'ll|\s+(?:will|(?:am|are|is) going to|promise to|can commit to))\s+(?:(?:\p{L}+ly|also|still|soon|then|just)\s+)*$")
        .expect("response commitment predicate regex")
});
static OFFER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s+(?:can|could)\s+(?:(?:\p{L}+ly|also|still|soon|then|just)\s+)*$")
        .expect("response offer predicate regex")
});
static DECLINED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s+(?:will not|won't|cannot|can't)\s+(?:(?:\p{L}+ly|also|still|soon|then|just)\s+)*$")
        .expect("declined response predicate regex")
});
static EXPERIENCE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^how (?:was|were|did)\b").expect("experience question regex"));
static FEEDBACK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:i|we)(?:'d| would) (?:love|welcome|appreciate) (?:your|some|any) feedback[.!]?$",
    )
    .expect("optional feedback invitation regex")
});

/// Read the grammatical subject before the observed response verb. A mention
/// of the addressee can instead govern an embedded request, so retain that
/// request's existing evidence rather than attributing it to the speaker.
pub(super) fn statement(
    prefix: &str,
    speaker: Actor,
    recipient: Actor,
) -> Option<(Act, Actor, bool)> {
    let prefix = prefix.trim_start();
    let lower = prefix.to_ascii_lowercase();
    let prefix = if lower.starts_with("if ") || lower.starts_with("unless ") {
        prefix.split_once(',')?.1.trim_start()
    } else {
        prefix
    };
    let subject = SUBJECT.captures(prefix)?;
    let predicate = subject.get(2)?.as_str();
    let first_person = matches!(subject[1].to_ascii_lowercase().as_str(), "i" | "we");
    if first_person
        && NOMINAL_REQUEST.is_match(predicate)
        && !predicate
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("to"))
    {
        return Some((Act::Request, recipient, false));
    }
    if ADDRESSED.is_match(predicate) {
        return None;
    }
    let actor = if first_person {
        speaker
    } else {
        Actor::ThirdParty
    };
    let declined = DECLINED.is_match(predicate);
    let act = if COMMITMENT.is_match(predicate) {
        Act::Commitment
    } else if OFFER.is_match(predicate) {
        Act::Offer
    } else {
        Act::Information
    };
    Some((act, actor, declined))
}

pub(super) fn joins_feedback(question: &str, invitation: &str) -> bool {
    EXPERIENCE.is_match(question) && question.ends_with('?') && FEEDBACK.is_match(invitation)
}

pub(super) fn feedback_invitation(text: &str) -> bool {
    FEEDBACK.is_match(text)
        || text.split_once('?').is_some_and(|(question, invitation)| {
            EXPERIENCE.is_match(question) && FEEDBACK.is_match(invitation.trim())
        })
}
