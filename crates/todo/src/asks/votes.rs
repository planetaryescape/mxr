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
    REQUEST,
    r"(?i)\b(please|kindly|can you|could you|would you|will you|you must|need you to|you need to)\b"
);
pattern!(
    PROMISE,
    r"(?i)\b(i(?:'ll| will)|we(?:'ll| will)|i promise to|i can commit to)\s+(send|provide|upload|sign|pay|book|contact|return|prepare|complete|deliver)\b"
);
pattern!(
    CANCELLED,
    r"(?i)\b(do not|don't|no longer (?:need|require)|cancelled|canceled|not required|need not|not necessary)\b"
);
pattern!(
    CONDITIONAL,
    r"(?i)\b(if|might|maybe|perhaps|would have|could have)\b"
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
    r"(?i)\b(sign (?:the|your|this|our|my|a) (?:[a-z]+\s+){0,2}(?:form|contract|agreement|waiver|document|lease|nda)|signature (?:required|requested)|awaiting your signature)\b"
);
pattern!(
    VERIFY,
    r"(?i)\b(?:verify|confirm|activate) (?:your |the |this )?(?:email(?: address)?|identity|account|phone number|registration)\b"
);
pattern!(
    SEND,
    r"(?i)\bsend (?:me |us |the |your |a |an |my |our )[^?!.\n]+\b"
);
pattern!(
    PROVIDE,
    r"(?i)\bprovide (?:me |us |the |your |a |an )[^?!.\n]+\b"
);
pattern!(UPLOAD, r"(?i)\bupload (?:the |your |a |an )[^?!.\n]+\b");
pattern!(BOOK, r"(?i)\bbook (?:the |your |a |an )[^?!.\n]+\b");
pattern!(
    REVIEW,
    r"(?i)\breview (?:the |your |this |my |our |a )[^?!.\n]+\b"
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
    r"(?i)\b(reply|respond|let me know|let us know|what do you think|are you available|does (?:this|that) work|can you confirm (?:if|whether))\b"
);

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
            .filter(|(_, d)| d.checked && inside(zone, span, d.zone, &d.span))
            .map(|(id, d)| (id, d.at)),
    );
    let reference = agreed_candidate(
        facts
            .references
            .iter()
            .enumerate()
            .filter(|(_, r)| inside(zone, span, r.zone, &r.span))
            .map(|(id, r)| (id, (r.kind.as_str(), r.value.as_str()))),
    );
    let actor = actor(text, facts, input);
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
    } else if CONDITIONAL.is_match(text) {
        AskState::Conditional
    } else {
        AskState::Pending
    };
    let promise = PROMISE.is_match(text) || inherited_promise(prefix);
    let promise_actor = if facts.owner_authored {
        Actor::Owner
    } else {
        Actor::Correspondent
    };
    let mut votes = Vec::new();
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
    let explicit = REQUEST.is_match(text) || promise || imperative;
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
        if pattern.is_match(text)
            && (explicit || matches!(action, ActionKind::Sign | ActionKind::Verify))
        {
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
        && (REPLY.is_match(text)
            || text.ends_with('?') && QUESTION.is_match(text) && !PLEASANTRY.is_match(text))
    {
        votes.push(vote(
            "reply",
            Act::Question,
            state,
            ActionKind::Reply,
            actor,
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

fn inherited_promise(prefix: &str) -> bool {
    let preceding = prefix
        .rsplit(['.', '?', '!', '\n', ';'])
        .next()
        .unwrap_or_default();
    let lower = preceding.trim_end().to_ascii_lowercase();
    (lower.ends_with(" and") || lower.ends_with(" then")) && PROMISE.is_match(preceding)
}

fn actor(text: &str, facts: &AskFacts, input: &AskInput<'_>) -> Actor {
    if facts.owner_authored {
        return Actor::Correspondent;
    }
    let lower = text.to_lowercase();
    let own_name = facts.owner_names.iter().any(|name| {
        !name.trim().is_empty() && lower.starts_with(&format!("{},", name.to_lowercase()))
    });
    let other_name = input
        .to
        .iter()
        .filter(|a| !(input.is_owner)(&a.email))
        .filter_map(|a| a.name.as_ref())
        .any(|name| !name.is_empty() && lower.starts_with(&format!("{},", name.to_lowercase())));
    if other_name {
        return Actor::ThirdParty;
    }
    if own_name && facts.recipient_role != RecipientRole::List {
        return Actor::Owner;
    }
    match facts.recipient_role {
        RecipientRole::AloneTo => Actor::Owner,
        RecipientRole::SharedTo if own_name => Actor::Owner,
        RecipientRole::SharedTo => Actor::Everyone,
        RecipientRole::Cc | RecipientRole::Bcc | RecipientRole::Unknown | RecipientRole::List => {
            Actor::ThirdParty
        }
    }
}
