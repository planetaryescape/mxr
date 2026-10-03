//! Mail that shows Messages as people with their topics (blueprint 22,
//! phase 3).
//!
//! * Samir Patel is one person with two topics: Contract renewal, where he
//!   replied with a long letter that quotes you and carries his Gmail
//!   signature (so it shows trimmed), and Launch checklist, where you're
//!   waiting on him.
//! * Samir and Ruth on Pricing copy is a group thread: its own row.
//! * Jon Bell asked a one-line question: a compact message.
//! * Iris Chen wrote and you answered "Thanks, on it.": Recent.
//! * Iris also wrote to Ruth with you only copied: that thread is in
//!   Updates, not Messages.

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageFlags, UnsubscribeMethod};

/// Messages `messages_demo_messages` returns.
pub(super) const MESSAGES_DEMO_MESSAGE_COUNT: usize = 11;

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-messages-{name}"))
}

fn person(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

fn message(
    from: &Address,
    to: &[&Address],
    cc: &[&Address],
    subject: &str,
    body: &str,
    date: DateTime<Utc>,
    sent: bool,
) -> DemoMessage {
    DemoMessage {
        from: from.clone(),
        to: to.iter().map(|a| (*a).clone()).collect(),
        cc: cc.iter().map(|a| (*a).clone()).collect(),
        snippet: body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect(),
        subject: subject.to_string(),
        body_text: body.to_string(),
        date,
        flags: if sent {
            MessageFlags::READ | MessageFlags::SENT
        } else {
            MessageFlags::empty()
        },
        has_attachments: false,
        category: 0,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: vec![if sent { "SENT" } else { "INBOX" }.to_string()],
        in_reply_to: None,
        references: Vec::new(),
    }
}

const SAMIR_LETTER: &str = "Hi Alex,\n\nThanks for sending the renewal draft over. I went through it with legal this morning and most of it is fine as written.\n\nTwo things changed on our side. The support tier moves from business hours to extended hours, and the notice period goes from 30 to 60 days. Both are in the redline.\n\nThe pricing schedule stays the same for the first year, with the usual 4% uplift after that.\n\nRollout risk: legal wants a signed copy before the 15th, so the new tier starts with the next billing period.\n\nCan you take a look and reply with the next concrete step?\n\nSamir";

const SAMIR_SIGNATURE_HTML: &str = r#"<div class="gmail_signature" data-smartmail="gmail_signature"><div dir="ltr">Samir Patel<br>Head of Partnerships, Launchpad<br>+44 20 7946 0102</div></div>"#;

/// Samir's letter as Gmail sends it: his text, his signature, then your
/// message quoted under `gmail_quote`.
fn samir_html(quoted: &str) -> String {
    let paragraphs: String = SAMIR_LETTER
        .split("\n\n")
        .map(|p| format!("<div>{p}</div><div><br></div>"))
        .collect();
    let quoted: String = quoted
        .split("\n\n")
        .map(|p| format!("<div>{}</div><div><br></div>", p.replace('\n', "<br>")))
        .collect();
    format!(
        r#"<div dir="ltr">{paragraphs}{SAMIR_SIGNATURE_HTML}</div><br><div class="gmail_quote"><div dir="ltr" class="gmail_attr">On Wed, Alex Demo &lt;alex@demo.mxr.local&gt; wrote:<br></div><blockquote class="gmail_quote" style="margin:0px 0px 0px 0.8ex;border-left:1px solid rgb(204,204,204);padding-left:1ex"><div dir="ltr">{quoted}</div></blockquote></div>"#
    )
}

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn messages_demo_messages(
    account_id: &AccountId,
    me: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let samir = person("Samir Patel", "samir@launchpad.example");
    let ruth = person("Ruth Vega", "ruth@keystone.example");
    let jon = person("Jon Bell", "jon@papertrail.example");
    let iris = person("Iris Chen", "iris@meridian.example");

    let mut built: Vec<(Envelope, MessageBody)> = Vec::with_capacity(MESSAGES_DEMO_MESSAGE_COUNT);
    let mut push = |message: DemoMessage, thread_id: &ThreadId| -> usize {
        let (envelope, body) =
            build_demo_msg(first_num + built.len(), account_id, thread_id, message);
        built.push((envelope, body));
        built.len() - 1
    };

    let contract = thread(account_id, "contract");
    let asked = "Hi Samir,\n\nHere's the renewal draft for next year. Could you check it with legal and let me know what changes on your side?\n\nAlex";
    push(
        message(me, &[&samir], &[], "Contract renewal", asked, now - Duration::days(2), true),
        &contract,
    );
    let quoted_plain: String = asked.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n");
    let letter = push(
        message(
            &samir,
            &[me],
            &[],
            "Re: Contract renewal",
            &format!(
                "{SAMIR_LETTER}\n\n-- \nSamir Patel\nHead of Partnerships, Launchpad\n\nOn Wed, Alex Demo <alex@demo.mxr.local> wrote:\n{quoted_plain}"
            ),
            now - Duration::hours(16),
            false,
        ),
        &contract,
    );

    let checklist = thread(account_id, "checklist");
    push(
        message(
            me,
            &[&samir],
            &[],
            "Launch checklist",
            "Samir, the launch checklist is in the shared folder. Can you tick off the partner items by Friday?",
            now - Duration::days(1),
            true,
        ),
        &checklist,
    );

    let pricing = thread(account_id, "pricing");
    push(
        message(me, &[&samir, &ruth], &[], "Pricing copy", "Here's the new pricing page copy. Does it read right to both of you?", now - Duration::days(4), true),
        &pricing,
    );
    push(
        message(&samir, &[me, &ruth], &[], "Re: Pricing copy", "Reads well to me. Ruth, the enterprise line is yours.", now - Duration::days(3), false),
        &pricing,
    );
    push(
        message(&ruth, &[me, &samir], &[], "Re: Pricing copy", "I'd say \"from\" before the enterprise price. Otherwise good to go. Alex, can you make the change?", now - Duration::days(2), false),
        &pricing,
    );

    let jon_thread = thread(account_id, "jon");
    push(
        message(me, &[&jon], &[], "Pricing copy for the docs", "Jon, I moved the pricing copy into the docs draft.", now - Duration::days(1), true),
        &jon_thread,
    );
    push(
        message(&jon, &[me], &[], "Re: Pricing copy for the docs", "Does the pricing copy read right to you?", now - Duration::hours(8), false),
        &jon_thread,
    );

    let iris_thread = thread(account_id, "iris");
    push(
        message(&iris, &[me], &[], "Incident note", "The incident note is ready for a read when you have a minute.", now - Duration::hours(30), false),
        &iris_thread,
    );
    push(
        message(me, &[&iris], &[], "Re: Incident note", "Thanks, on it.", now - Duration::hours(28), true),
        &iris_thread,
    );

    // Only copied, never wrote in it: Updates, not Messages.
    push(
        message(&iris, &[&ruth], &[me], "Offsite dates", "Ruth, the offsite is booked for the 20th and 21st. Alex, copying you so you have the dates.", now - Duration::hours(5), false),
        &thread(account_id, "offsite"),
    );

    // Samir's reply as Gmail sends it, quote and signature marked.
    if let Some((_, body)) = built.get_mut(letter) {
        body.text_html = Some(samir_html(asked));
    }
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_matches_the_mail_built() {
        let account_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let me = Address {
            name: Some("Alex Demo".to_string()),
            email: "alex@demo.mxr.local".to_string(),
        };
        let built = messages_demo_messages(&account_id, &me, Utc::now(), 1);
        assert_eq!(built.len(), MESSAGES_DEMO_MESSAGE_COUNT);
        let html = built[1].1.text_html.as_deref().unwrap();
        assert!(html.contains("gmail_quote") && html.contains("gmail_signature"));
    }
}
