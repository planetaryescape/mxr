//! Mail that shows one email in two modes, and a first-time sender.
//!
//! * You asked Sam Okafor, your landlord, about the lease renewal; he sent
//!   it back to sign within two days and asked if you're around Thursday.
//!   It's your turn in Messages and a sign to-do in To do, so done in
//!   Messages leaves the to-do open and the email in the inbox.
//! * Noor Haddad writes for the first time: her row on Now asks once
//!   where her mail belongs (D117).

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageFlags, UnsubscribeMethod};

/// Messages `modes_demo_messages` returns.
pub(super) const MODES_DEMO_MESSAGE_COUNT: usize = 3;

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-modes-{name}"))
}

fn message(
    from: Address,
    to: Address,
    subject: &str,
    body: String,
    date: DateTime<Utc>,
) -> DemoMessage {
    DemoMessage {
        from,
        to: vec![to],
        cc: Vec::new(),
        snippet: body.chars().take(120).collect(),
        subject: subject.to_string(),
        body_text: body,
        date,
        flags: MessageFlags::empty(),
        has_attachments: false,
        category: 0,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: vec!["INBOX".to_string()],
        in_reply_to: None,
        references: Vec::new(),
    }
}

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn modes_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let sam = Address {
        name: Some("Sam Okafor".to_string()),
        email: "sam@okafor-lettings.example".to_string(),
    };
    let noor = Address {
        name: Some("Noor Haddad".to_string()),
        email: "noor@tidewater.example".to_string(),
    };
    let lease = thread(account_id, "lease");
    let sign_by = (now + Duration::days(2)).format("%-d %B %Y").to_string();

    let mut built = Vec::with_capacity(MODES_DEMO_MESSAGE_COUNT);
    let mut push = |message: DemoMessage, thread_id: &ThreadId| {
        let (envelope, body) =
            build_demo_msg(first_num + built.len(), account_id, thread_id, message);
        built.push((envelope, body));
    };

    let mut asked = message(
        self_addr.clone(),
        sam.clone(),
        "Lease renewal",
        "Hi Sam,\n\nIs the renewal for next year ready? Happy to sign whenever it is.\n\nAlex"
            .to_string(),
        now - Duration::days(2),
    );
    asked.flags = MessageFlags::READ | MessageFlags::SENT;
    asked.label_provider_ids = vec!["SENT".to_string()];
    push(asked, &lease);

    push(
        message(
            sam,
            self_addr.clone(),
            "Re: Lease renewal",
            format!(
                "Hi Alex,\n\nThe renewal for next year is attached. Please sign and send it back by {sign_by}.\n\nAlso, are you around Thursday? The boiler engineer wants to come by in the morning.\n\nSam"
            ),
            now - Duration::hours(3),
        ),
        &lease,
    );

    push(
        message(
            noor,
            self_addr.clone(),
            "Introduction from Tidewater",
            "Hi Alex,\n\nMaya suggested I get in touch about the sync engine talk. Would you have twenty minutes next week?\n\nNoor".to_string(),
            now - Duration::hours(1),
        ),
        &thread(account_id, "noor"),
    );

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
        let built = modes_demo_messages(&account_id, &me, Utc::now(), 1);
        assert_eq!(built.len(), MODES_DEMO_MESSAGE_COUNT);
        assert_eq!(built[0].0.thread_id, built[1].0.thread_id);
    }
}
