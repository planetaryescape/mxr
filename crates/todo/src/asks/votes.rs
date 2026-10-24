use super::{candidates::inside, AskInput};
use mxr_core::asks::*;
use once_cell::sync::Lazy;
use regex::Regex;

macro_rules! pattern {
    ($name:ident, $pattern:literal) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($pattern).expect("ask vote regex"));
    };
}
pattern!(
    NEEDS_ATTENTION,
    r"(?i)\b(?:requires?|needs?)\s+(?:some\s+|your\s+)?attention\b"
);
pattern!(
    REQUEST,
    r"(?i)\b(please|kindly|can you|could you|would you|will you|you must|need you to|you need to)\b"
);
pattern!(
    PROMISE,
    r"(?i)\b(i(?:'ll| will)|we(?:'ll| will)|i promise to|i can commit to)\s+(send|provide|upload|sign|pay|book|contact|return|prepare|complete|deliver|go through|update)\b"
);
pattern!(OFFER_SPEAKER, r"(?i)\b(?:i|we) (?:can|could)\s+\p{L}");
pattern!(FUTURE_SPEAKER, r"(?i)\b(?:i|we)(?:'ll| will)\s+\p{L}");
pattern!(
    CANCELLED,
    r"(?i)\b(do not|don't|no longer (?:need|require)|cancelled|canceled|not required|need not|no need to|not necessary)\b"
);
pattern!(
    CONDITIONAL,
    r"(?i)(?:^if\b|\b(?:might|maybe|perhaps|would have|could have)\b|\blet (?:me|us) know if you have (?:any )?questions\b)"
);
pattern!(IF_WORD, r"(?i)\b(?:if|unless)\b");
pattern!(
    QUESTION_IF,
    r"(?i)\b(?:let (?:me|us) know|tell (?:me|us)|confirm|see|check|(?:do|would) you mind|even)\s+if\b"
);

pattern!(
    DONE,
    r"(?i)\b(i(?:'ve| have) (?:sent|paid|signed|uploaded|provided)|already (?:sent|paid|signed|done)|has been (?:signed|completed))\b"
);
pattern!(
    PLEASANTRY,
    r"(?i)^(?:hi\b|hello\b|dear\b|thanks?\b|thank you\b|how are you\b|hope you(?:'re| are) well\b)"
);
pattern!(
    SIGN,
    r"(?i)\b(sign (?:the|your|this|our|my|a) (?:[a-z]+\s+){0,2}(?:form|contract|agreement|waiver|document|lease|nda)|signature (?:required|requested)|awaiting your signature|please sign\b|review and sign\b|(?:form|document|contract|agreement) must be signed)\b|^sign:\s*\S+"
);
pattern!(
    VERIFY,
    r"(?i)\b(?:verify|confirm|activate) (?:your |the |this )?(?:email(?: address)?|identity|account|phone number|registration)\b"
);
pattern!(
    SEND,
    r"(?i)\bsend (?:me |us |you |the |your |a |an |my |our )[^?!.\n]+\b"
);
pattern!(
    PROVIDE,
    r"(?i)\b(?:provide (?:me |us |the |your |a |an )[^?!.\n]+|confirm your delivery address)\b"
);
pattern!(UPLOAD, r"(?i)\bupload (?:the |your |a |an )[^?!.\n]+\b");
pattern!(BOOK, r"(?i)\bbook (?:the |your |a |an )[^?!.\n]+\b");
pattern!(
    REVIEW,
    r"(?i)\b(?:(?:review|go through|update) (?:the |your |this |my |our |a )[^?!.\n]+|take a look)\b"
);
pattern!(
    PAY_PROMISE,
    r"(?i)\bpay (?:the |your |my |our |a |invoice\b|[£$€R])[^?!.\n]*"
);
pattern!(
    QUESTION,
    r"(?i)^(?:what|when|where|who|which|how|does|do you|are you|is|would (?:this|that))\b"
);
pattern!(
    CONTACT,
    r"(?i)\bcontact (?:the |your |our |me\b|us\b)[^?!.\n]*"
);
pattern!(
    REPLY,
    r"(?i)\b(reply|respond|let me know|let us know|what do you think|are you available|does (?:this|that) work|(?:can|could|would) you confirm|tell me (?:if|whether)|(?:share|send)(?: me| us)? your availability|would you have|shall we|are you (?:around|free))\b"
);

pattern!(
    SALUTATION,
    r"^(?:(?P<greeting>(?i:hi|hello|hey|dear|howzit|good morning|good afternoon|good evening))[ \t]+)?(?P<name>\p{Lu}[\p{L}'’-]+(?:[ \t]+\p{Lu}[\p{L}'’-]+){0,2})(?P<separator>[,:])(?P<rest>.*)$"
);

pattern!(
    RECIPIENT_ROLE,
    r"(?i)^(?:valued )?(?:customer|member|ratepayer|subscriber|resident|tenant|account holder|card ?holder|policy ?holder)$"
);
pattern!(
    REPLY_WITHDRAWAL,
    r"(?i)^(?:no need to|do not|don't|need not) (?:reply|respond|confirm)(?: to (?:me|us|this (?:email|message|question)))?[.!]?$"
);

pattern!(PURPOSE, r"(?i)\s+so (?:that )?(?:i|we) (?:can|will|know)\b");
pattern!(
    CHANGE,
    r"(?i)\b(?:make the change|check (?:the|your|this) [^?!.]+|try [^?!.]+)\b"
);
pattern!(
    PUBLIC_EVENT,
    r"(?i)\b(?:product webinar|marketing webinar|sales webinar|product demo|promotional event)\b"
);
pattern!(RSVP, r"(?i)\brsvp\b");

pub(super) fn label(
    text: &str,
    prefix: &str,
    zone: usize,
    span: &AskSpan,
    facts: &AskFacts,
    doc: &DocumentReading,
    input: &AskInput<'_>,
) -> Vec<AskVote> {
    let amount = agreed_candidate(
        facts
            .amounts
            .iter()
            .enumerate()
            .filter(|(_, a)| inside(zone, span, a.zone, &a.span))
            .map(|(id, a)| (id, (a.minor, a.currency.as_str()))),
    );
    let due = agreed_candidate(
        facts
            .dates
            .iter()
            .enumerate()
            .filter(|(_, d)| inside(zone, span, d.zone, &d.span))
            .filter(|(_, d)| {
                doc.kind != DocumentKind::FailedPayment
                    || !facts.zones[d.zone].text[d.span.start..d.span.end]
                        .to_ascii_lowercase()
                        .starts_with("expires")
            })
            .map(|(id, d)| (id, d.at)),
    );
    let reference = agreed_candidate(
        facts
            .references
            .iter()
            .enumerate()
            .filter(|(_, r)| inside(zone, span, r.zone, &r.span))
            .map(|(id, r)| (id, (r.kind.as_str(), r.value.as_str()))),
    )
    .or_else(|| {
        // A reference header governs following units until another reference
        // appears. Paragraph boundaries end that context.
        facts
            .references
            .iter()
            .enumerate()
            .rev()
            .find(|(_, r)| {
                r.zone == zone
                    && r.span.end <= span.start
                    && !facts.zones[zone].text[r.span.end..span.start].contains("\n\n")
            })
            .map(|(id, _)| id)
    });
    let actor = actor(text, prefix, facts, input);
    // Purpose clauses explain the sender's next step, not a second owner action.
    let action_text = PURPOSE.find(text).map_or(text, |m| &text[..m.start()]);
    let base = AskTags {
        act: Act::Information,
        actor,
        state: AskState::Pending,
        action: ActionKind::Other,
        amount,
        due,
        reference,
    };
    let vote = |rule: &str, act, state, action, actor| AskVote {
        rule: rule.into(),
        tags: AskTags {
            act,
            actor,
            state,
            action,
            ..base.clone()
        },
    };
    // Structural gates apply to subjects too. A quoted request's subject
    // cannot silently become its own delegation.
    if input.list_mail || input.bulk {
        return vec![vote(
            "genre",
            Act::Offer,
            AskState::Pending,
            ActionKind::Other,
            Actor::Nobody,
        )];
    }
    if facts.zones[zone].kind == ZoneKind::Subject
        && input.body.is_some()
        && !facts
            .zones
            .iter()
            .any(|z| z.kind == ZoneKind::Authored && !z.text.trim().is_empty())
    {
        return vec![vote(
            "context_only",
            Act::Information,
            AskState::Pending,
            ActionKind::Other,
            Actor::Nobody,
        )];
    }
    let state = if CANCELLED.is_match(text) {
        AskState::Cancelled
    } else if DONE.is_match(text) {
        AskState::Done
    } else if !REQUEST.is_match(text) && inherited_offer(prefix) {
        AskState::Hypothetical
    } else if conditional_scope(text)
        || conditional_scope(
            prefix
                .rsplit(['.', '?', '!', '\n'])
                .next()
                .unwrap_or_default(),
        )
    {
        AskState::Conditional
    } else {
        AskState::Pending
    };
    let promise = PROMISE.is_match(text) || (!REQUEST.is_match(text) && inherited_promise(prefix));
    let promise_actor = if facts.owner_authored {
        Actor::Owner
    } else {
        Actor::Correspondent
    };
    let mut votes = Vec::new();
    if super::completion::required(text)
        && (REQUEST.is_match(text) || super::completion::includes_delivery(text))
    {
        votes.push(vote(
            "required_completion",
            Act::Obligation,
            state,
            ActionKind::Other,
            actor,
        ));
    }

    match doc.status {
        PaymentStatus::Complete | PaymentStatus::Credit | PaymentStatus::Automatic => {
            votes.push(vote(
                "document_state",
                Act::Information,
                AskState::Done,
                ActionKind::Pay,
                Actor::Nobody,
            ));
        }
        PaymentStatus::Due | PaymentStatus::PastDue | PaymentStatus::Failed
            if doc.required_fields_present =>
        {
            votes.push(vote(
                "document_required_fields",
                if promise {
                    Act::Commitment
                } else {
                    Act::Obligation
                },
                state,
                if doc.status == PaymentStatus::Failed {
                    ActionKind::FixPayment
                } else {
                    ActionKind::Pay
                },
                if promise { promise_actor } else { actor },
            ));
        }
        _ if doc.kind == DocumentKind::Offer => {
            votes.push(vote(
                "offer",
                Act::Offer,
                state,
                ActionKind::Other,
                Actor::Nobody,
            ));
        }
        _ => {}
    }
    let lower = text.to_ascii_lowercase();
    let imperative = [
        "send ", "provide ", "upload ", "book ", "contact ", "review ",
    ]
    .iter()
    .any(|verb| lower.starts_with(verb));
    let explicit = REQUEST.is_match(action_text) || promise || imperative;
    for (pattern, action, rule) in [
        (&*SIGN, ActionKind::Sign, "sign"),
        (&*VERIFY, ActionKind::Verify, "verify"),
        (&*SEND, ActionKind::Send, "send"),
        (&*PROVIDE, ActionKind::Provide, "provide"),
        (&*UPLOAD, ActionKind::Upload, "upload"),
        (&*BOOK, ActionKind::Book, "book"),
        (&*REVIEW, ActionKind::Other, "review"),
        (&*CONTACT, ActionKind::Contact, "contact"),
    ] {
        if pattern.is_match(action_text)
            && (explicit || matches!(action, ActionKind::Sign | ActionKind::Verify))
        {
            if matches!(
                action,
                ActionKind::Send | ActionKind::Provide | ActionKind::Upload
            ) && super::completion::includes_delivery(text)
            {
                continue;
            }
            if action == ActionKind::Verify
                && (crate::detect::SECURITY.is_match(text)
                    || crate::detect::SECURITY.is_match(input.subject)
                    || crate::detect::SECURITY
                        .is_match(prefix.rsplit("\n\n").next().unwrap_or_default()))
            {
                votes.push(vote(
                    "security_notice",
                    Act::Information,
                    state,
                    action,
                    Actor::Nobody,
                ));
                continue;
            }
            if action == ActionKind::Other && doc.kind == DocumentKind::FailedPayment {
                continue;
            }
            if action == ActionKind::Send && doc.kind == DocumentKind::Bill
                || action == ActionKind::Other && REPLY.is_match(action_text)
            {
                continue;
            }
            let (act, actor) = if promise {
                (
                    Act::Commitment,
                    if facts.owner_authored {
                        Actor::Owner
                    } else {
                        Actor::Correspondent
                    },
                )
            } else {
                (Act::Request, actor)
            };
            votes.push(vote(rule, act, state, action, actor));
        }
    }
    if promise && PAY_PROMISE.is_match(text) {
        votes.push(vote(
            "promise_pay",
            Act::Commitment,
            state,
            ActionKind::Pay,
            if facts.owner_authored {
                Actor::Owner
            } else {
                Actor::Correspondent
            },
        ));
    }
    if votes.is_empty()
        && (REQUEST.is_match(action_text) || imperative)
        && CHANGE.is_match(action_text)
        && !REPLY.is_match(action_text)
    {
        votes.push(vote(
            "change",
            Act::Request,
            state,
            ActionKind::Other,
            actor,
        ));
    }
    if votes.is_empty() && RSVP.is_match(action_text) {
        let public = !input.reply_capable
            || PUBLIC_EVENT.is_match(action_text)
            || PUBLIC_EVENT.is_match(prefix.rsplit("\n\n").next().unwrap_or_default());
        votes.push(vote(
            if public {
                "public_event_offer"
            } else {
                "authored_rsvp"
            },
            if public { Act::Offer } else { Act::Request },
            state,
            ActionKind::Reply,
            if public { Actor::Nobody } else { actor },
        ));
    }
    if votes.is_empty() && super::reply::feedback_invitation(text) {
        votes.push(vote(
            "optional_feedback",
            Act::Offer,
            state,
            ActionKind::Reply,
            Actor::Nobody,
        ));
    }
    if votes.is_empty()
        && (REPLY.is_match(text)
            || text.ends_with('?') && QUESTION.is_match(text) && !PLEASANTRY.is_match(text))
    {
        // Attach the observed response to its own subject. Preserve requests
        // with embedded futures, and reuse the same coordinated promise/offer
        // attachment as non-reply actions without splitting review-and-reply.
        let speech = salutation(text).map_or(text, |(_, rest)| rest).trim_start();
        let statement = (!speech.ends_with('?') && !REQUEST.is_match(speech))
            .then(|| {
                let prefix = &speech[..REPLY.find(speech)?.start()];
                let (mut act, subject, declined) =
                    super::reply::statement(prefix, promise_actor, actor)?;
                if act == Act::Information && !declined {
                    if inherited_promise(prefix) {
                        act = Act::Commitment;
                    } else if inherited_offer(prefix) {
                        act = Act::Offer;
                    }
                }
                Some((act, subject, declined))
            })
            .flatten();
        votes.push(vote(
            "reply",
            statement.map_or(Act::Question, |(act, _, _)| act),
            if statement.is_some_and(|(_, _, declined)| declined) {
                AskState::Cancelled
            } else {
                state
            },
            ActionKind::Reply,
            statement.map_or(actor, |(_, subject, _)| subject),
        ));
    }
    if votes.is_empty() && (PLEASANTRY.is_match(text) || state != AskState::Pending) {
        votes.push(vote(
            "non_action",
            Act::Information,
            state,
            ActionKind::Other,
            Actor::Nobody,
        ));
    }
    votes
}

fn agreed_candidate<T: PartialEq>(
    mut candidates: impl Iterator<Item = (usize, T)>,
) -> Option<usize> {
    let (first, value) = candidates.next()?;
    candidates.all(|(_, other)| other == value).then_some(first)
}

fn inherited_offer(prefix: &str) -> bool {
    let preceding = prefix
        .rsplit(['.', '?', '!', '\n', ';'])
        .next()
        .unwrap_or_default();
    let lower = preceding.trim_end().to_ascii_lowercase();
    (lower.ends_with(" and") || lower.ends_with(" then")) && OFFER_SPEAKER.is_match(preceding)
}

fn inherited_promise(prefix: &str) -> bool {
    let preceding = prefix
        .rsplit(['.', '?', '!', '\n', ';'])
        .next()
        .unwrap_or_default();
    let lower = preceding.trim_end().to_ascii_lowercase();
    (lower.ends_with(" and") || lower.ends_with(" then")) && FUTURE_SPEAKER.is_match(preceding)
}

fn actor(text: &str, prefix: &str, facts: &AskFacts, input: &AskInput<'_>) -> Actor {
    if facts.owner_authored {
        return Actor::Correspondent;
    }
    // An explicit salutation cannot assign another person's request to the
    // account owner merely because the email has only one To recipient.
    let addressed = salutation(text).map(|(name, _)| name).or_else(|| {
        let mut paragraphs = prefix.rsplitn(2, "\n\n");
        let current = paragraphs.next().unwrap_or_default();
        salutation_header(current).or_else(|| {
            // A name-only salutation may introduce the next paragraph, but an
            // addressed action in another paragraph cannot veto this unit.
            let previous = paragraphs.next()?.rsplit("\n\n").next()?;
            (previous
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
                == 1)
                .then(|| salutation_header(previous))
                .flatten()
        })
    });
    if let Some(name) = addressed {
        return if facts.owner_names.iter().any(|own| {
            own.eq_ignore_ascii_case(name)
                || own
                    .split_whitespace()
                    .next()
                    .is_some_and(|first| first.eq_ignore_ascii_case(name))
        }) {
            Actor::Owner
        } else {
            Actor::ThirdParty
        };
    }
    let lower = text.to_lowercase();
    let own_name = facts
        .owner_names
        .iter()
        .any(|name| named_addressee(&lower, name));
    let other_name = input
        .to
        .iter()
        .filter(|a| !(input.is_owner)(&a.email))
        .filter_map(|a| a.name.as_ref())
        .any(|name| named_addressee(&lower, name));
    if other_name {
        return Actor::ThirdParty;
    }
    if own_name && facts.recipient_role != RecipientRole::List {
        return Actor::Owner;
    }
    match facts.recipient_role {
        RecipientRole::AloneTo => Actor::Owner,
        RecipientRole::SharedTo
            if own_name
                || facts
                    .owner_names
                    .iter()
                    .any(|name| named_addressee(&prefix.trim_start().to_lowercase(), name)) =>
        {
            Actor::Owner
        }
        RecipientRole::SharedTo => Actor::Everyone,
        RecipientRole::Cc | RecipientRole::Bcc | RecipientRole::Unknown | RecipientRole::List => {
            Actor::ThirdParty
        }
    }
}

/// Unsupported informational prose is not ask uncertainty. Keep only an
/// observed request/commitment or incomplete consequential document reading.
pub(super) fn eligible_uncertainty(text: &str, doc: &DocumentReading) -> bool {
    NEEDS_ATTENTION.is_match(text)
        || REQUEST.is_match(text)
        || PROMISE.is_match(text)
        || matches!(
            doc.kind,
            DocumentKind::Bill | DocumentKind::Statement | DocumentKind::FailedPayment
        )
        || SIGN.is_match(text)
        || VERIFY.is_match(text)
}

pub(super) fn withdraws_previous_reply(text: &str) -> bool {
    REPLY_WITHDRAWAL.is_match(text.trim())
}

fn salutation_header(part: &str) -> Option<&str> {
    part.lines()
        .filter(|line| !line.trim().is_empty())
        .find_map(|line| {
            salutation(line).and_then(|(name, rest)| rest.trim().is_empty().then_some(name))
        })
}

fn salutation(text: &str) -> Option<(&str, &str)> {
    let captures = SALUTATION.captures(text.trim())?;
    let name = captures.name("name")?.as_str();
    // A colon alone introduces a heading, not an observed person's name.
    // Generic recipient roles likewise do not name a different actor.
    if captures.name("greeting").is_none() && captures.name("separator")?.as_str() == ":"
        || RECIPIENT_ROLE.is_match(name)
    {
        return None;
    }
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "hi" | "hello"
            | "hey"
            | "dear"
            | "good morning"
            | "good afternoon"
            | "good evening"
            | "thanks"
            | "thank you"
            | "please"
            | "kindly"
            | "fyi"
            | "invoice"
            | "account"
            | "order"
            | "reference"
            | "policy"
            | "date"
            | "amount"
    ) {
        return None;
    }
    let rest = captures.name("rest")?.as_str();
    (rest.trim().is_empty() || REQUEST.is_match(rest) || REPLY.is_match(rest))
        .then_some((name, rest))
}

fn named_addressee(text: &str, name: &str) -> bool {
    let name = name.trim().to_lowercase();
    !name.is_empty()
        && (text.starts_with(&format!("{name},"))
            || name
                .split_whitespace()
                .next()
                .is_some_and(|first| text.starts_with(&format!("{first},"))))
}

fn conditional_scope(text: &str) -> bool {
    CONDITIONAL.is_match(text)
        || IF_WORD.find_iter(text).any(|condition| {
            !QUESTION_IF.find_iter(text).any(|question| {
                question.start() <= condition.start() && question.end() >= condition.end()
            })
        })
}
