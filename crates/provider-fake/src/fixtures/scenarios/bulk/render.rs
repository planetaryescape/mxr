//! Turns a thread number and a position into the mail the demo serves: who
//! writes, what they say, when, and how the provider files it. Pure
//! arithmetic over the family's data, so any page can be generated from its
//! index alone.

use super::pools::{first_name, SUBJECT_POOL};
use super::{
    asks_recent, counterpart, family_for, observer, sender_of, variant_for, Attachments, Dating,
    Family, Senders, Speaker, Subjects, Unsubscribe, MINUTES_BETWEEN_MESSAGES,
};
use chrono::{DateTime, Duration, Utc};
use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};

/// One generated message, before the demo builder gives it ids.
pub(crate) struct Rendered {
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub snippet: String,
    pub body: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub has_attachments: bool,
    /// The look of the mail, read by the attachment and HTML builders.
    pub style: usize,
    pub unsubscribe: UnsubscribeMethod,
    pub provider_labels: Vec<String>,
    /// The message replies to the one before it, in its headers.
    pub reply_headers: bool,
}

/// Minutes between the seed clock and a thread's first message.
fn offset_minutes(family: &Family, thread_num: usize) -> i64 {
    let spread = (thread_num as i64 * 37) % (365 * 24 * 60);
    match family.dating {
        Dating::Spread => spread,
        Dating::Asks if asks_recent(thread_num) => spread % (6 * 60),
        Dating::Asks => 60 * 24 * 60 + spread % (300 * 24 * 60),
    }
}

/// The words a template may name.
struct Fill {
    name: String,
    other: String,
    topic: String,
    org: String,
    reference: String,
    amount: String,
    date: String,
    due: String,
    n: String,
}

impl Fill {
    fn apply(&self, template: &str) -> String {
        template
            .replace("{name}", &self.name)
            .replace("{other}", &self.other)
            .replace("{topic}", &self.topic)
            .replace("{org}", &self.org)
            .replace("{ref}", &self.reference)
            .replace("{amount}", &self.amount)
            .replace("{date}", &self.date)
            .replace("{due}", &self.due)
            .replace("{n}", &self.n)
    }
}

fn address((name, email): (&str, &str)) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

/// The first 120 characters of the text on one line, as a mail client's
/// preview shows.
fn snippet_of(body: &str) -> String {
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

/// The mail at position `reply_idx` of thread `thread_num`, seeded at `now`.
/// `thread_len` is the thread's full length even when the demo's size cuts
/// it short, because dates and "the last message" read it.
pub(crate) fn render(
    me: &Address,
    now: DateTime<Utc>,
    thread_num: usize,
    reply_idx: usize,
    thread_len: usize,
) -> Rendered {
    let family = family_for(thread_num);
    let turn = &family.turns[reply_idx];
    let sent = turn.from == Speaker::You;
    let (variant_index, _) = variant_for(family, thread_num);
    let (variant, _) = family.variant(variant_index);

    // Whom the message is between.
    let (from, to, cc, speaker, listener) = match family.senders {
        Senders::People { copied: true } => {
            let from =
                sender_of(family, thread_num, reply_idx).unwrap_or_else(|| counterpart(thread_num));
            let to = if reply_idx.is_multiple_of(2) {
                counterpart(thread_num)
            } else {
                super::pools::DEMO_PEOPLE[(thread_num + 5) % super::pools::DEMO_PEOPLE.len()]
            };
            (address(from), vec![address(to)], vec![me.clone()], from, to)
        }
        Senders::People { copied: false } => {
            let peer = counterpart(thread_num);
            let cc = if family.observer_cc {
                vec![address(observer(thread_num))]
            } else {
                Vec::new()
            };
            if sent {
                (me.clone(), vec![address(peer)], cc, peer, peer)
            } else {
                (address(peer), vec![me.clone()], cc, peer, peer)
            }
        }
        Senders::Pool(_) | Senders::ByVariant(_) => {
            let sender = sender_of(family, thread_num, reply_idx).unwrap_or(("", ""));
            (
                address(sender),
                vec![me.clone()],
                Vec::new(),
                sender,
                sender,
            )
        }
    };

    let offset = offset_minutes(family, thread_num);
    let date = now
        - Duration::minutes(offset + (thread_len - reply_idx) as i64 * MINUTES_BETWEEN_MESSAGES);
    let root = SUBJECT_POOL[thread_num % SUBJECT_POOL.len()];
    let fill = Fill {
        name: first_name(speaker.0).to_string(),
        other: match family.senders {
            Senders::People { copied: true } => first_name(listener.0).to_string(),
            _ => first_name(observer(thread_num).0).to_string(),
        },
        topic: if family.subjects == Subjects::Pool {
            root.to_string()
        } else {
            String::new()
        },
        org: speaker.0.to_string(),
        reference: format!(
            "{}-{}",
            family.ref_prefix,
            10_000 + (thread_num * 7919) % 90_000
        ),
        amount: format!(
            "£{}.{:02}",
            12 + (thread_num * 31) % 480,
            (thread_num * 17) % 100
        ),
        date: date.format("%-d %B").to_string(),
        due: (date + Duration::days(family.due_days + (thread_num % 5) as i64))
            .format("%A %-d %B")
            .to_string(),
        n: (2 + thread_num % 38).to_string(),
    };

    let subject = match family.subjects {
        Subjects::Pool => reply_prefixed(root.to_string(), reply_idx),
        Subjects::Thread => reply_prefixed(fill.apply(variant.subjects[0]), reply_idx),
        Subjects::PerTurn => fill.apply(variant.subjects[reply_idx]),
    };
    let body = if sent {
        fill.apply(family.sent_replies[(thread_num + reply_idx) % family.sent_replies.len()])
    } else {
        fill.apply(variant.bodies[family.inbound_before(reply_idx)])
    };

    let old_enough_to_read = offset > 24 * 60;
    let unread = !sent && !old_enough_to_read && reply_idx + 1 == thread_len;
    let starred = thread_num.is_multiple_of(29)
        || (family.subjects == Subjects::Pool && root.contains("Security"));
    let archived = family
        .archived_every
        .is_some_and(|every| thread_num.is_multiple_of(every));
    let mut flags = if unread {
        MessageFlags::empty()
    } else {
        MessageFlags::READ
    };
    if sent {
        flags |= MessageFlags::SENT | MessageFlags::READ;
    }
    if starred {
        flags |= MessageFlags::STARRED;
    }
    if family.spam_folder {
        flags |= MessageFlags::SPAM;
    }
    if archived {
        flags |= MessageFlags::ARCHIVED;
    }

    let mut provider_labels = Vec::new();
    provider_labels.push(
        if sent {
            "SENT"
        } else if flags.contains(MessageFlags::SPAM) {
            "SPAM"
        } else if flags.contains(MessageFlags::ARCHIVED) {
            "ARCHIVE"
        } else {
            "INBOX"
        }
        .to_string(),
    );
    if unread {
        provider_labels.push("UNREAD".to_string());
    }
    if starred {
        provider_labels.push("STARRED".to_string());
    }
    if let Some(label) = (family.provider_label)(thread_num) {
        provider_labels.push(label.to_string());
    }
    if family.waiting && !sent && reply_idx + 1 == thread_len {
        provider_labels.push("waiting".to_string());
    }

    let unsubscribe = match family.unsubscribe {
        Unsubscribe::None => UnsubscribeMethod::None,
        Unsubscribe::Lists if thread_num.is_multiple_of(2) => UnsubscribeMethod::OneClick {
            url: format!("https://lists.demo.mxr.local/unsubscribe/{thread_num}"),
        },
        Unsubscribe::Lists => UnsubscribeMethod::Mailto {
            address: "unsubscribe@lists.demo.mxr.local".to_string(),
            subject: Some(format!("unsubscribe-{thread_num}")),
        },
        Unsubscribe::Promo => UnsubscribeMethod::OneClick {
            url: format!("https://promo.demo.mxr.local/unsubscribe/{thread_num}"),
        },
    };
    let has_attachments = match family.attachments {
        Attachments::None => false,
        Attachments::Always => true,
        Attachments::FirstTurn => reply_idx == 0,
        Attachments::EveryNth(every) => thread_num.is_multiple_of(every),
    } || (family.brief_every_41 && thread_num.is_multiple_of(41));

    Rendered {
        from,
        to,
        cc,
        subject,
        snippet: snippet_of(&body),
        body,
        date,
        flags,
        has_attachments,
        style: (family.style)(thread_num),
        unsubscribe,
        provider_labels,
        reply_headers: family.reply_headers,
    }
}

/// "Re: " on every message after the first.
fn reply_prefixed(subject: String, reply_idx: usize) -> String {
    if reply_idx == 0 {
        subject
    } else {
        format!("Re: {subject}")
    }
}
