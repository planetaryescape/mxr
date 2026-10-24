//! Reply wording distinguishes a speaker's promise or offer from a recipient's turn.
use mxr_core::asks::Act;
use once_cell::sync::Lazy;
use regex::Regex;

static COMMITMENT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)^(?:i(?:'ll| will)|we(?:'ll| will)|i promise to|i can commit to)\s+(?:reply|respond)\b"
).expect("reply commitment regex")
});
static OFFER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:i|we) (?:can|could)\s+(?:reply|respond)\b").expect("reply offer regex")
});
static EXPERIENCE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^how (?:was|were|did)\b").expect("experience question regex"));
static FEEDBACK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:i|we)(?:'d| would) (?:love|welcome|appreciate) (?:your|some|any) feedback[.!]?$",
    )
    .expect("optional feedback invitation regex")
});

pub(super) fn speaker_act(text: &str) -> Option<Act> {
    if COMMITMENT.is_match(text) {
        Some(Act::Commitment)
    } else if OFFER.is_match(text) {
        Some(Act::Offer)
    } else {
        None
    }
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
