use chrono::{Duration, Utc};
use mxr_core::id::*;
use mxr_core::types::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FixtureDataset {
    pub envelopes: Vec<Envelope>,
    pub bodies: HashMap<String, MessageBody>,
    pub labels: Vec<Label>,
}

impl FixtureDataset {
    pub fn canonical(account_id: &AccountId) -> Self {
        let (envelopes, bodies, labels) = generate_fixtures(account_id);
        Self {
            envelopes,
            bodies,
            labels,
        }
    }

    pub fn unread_message(&self) -> Option<&Envelope> {
        self.envelopes
            .iter()
            .find(|envelope| !envelope.flags.contains(MessageFlags::READ))
    }

    pub fn attachment_message(&self) -> Option<(&Envelope, &MessageBody)> {
        self.envelopes.iter().find_map(|envelope| {
            self.bodies
                .get(&envelope.provider_id)
                .filter(|body| !body.attachments.is_empty())
                .map(|body| (envelope, body))
        })
    }
}

pub fn sample_draft(account_id: AccountId) -> Draft {
    Draft {
        id: DraftId::new(),
        account_id,
        in_reply_to: None,
        to: vec![Address {
            name: Some("Recipient".to_string()),
            email: "recipient@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Conformance test draft".to_string(),
        body_markdown: "Hello from conformance test.".to_string(),
        attachments: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn sample_from_address() -> Address {
    Address {
        name: Some("mxr Conformance".to_string()),
        email: "sender@example.com".to_string(),
    }
}

pub fn generate_fixtures(
    account_id: &AccountId,
) -> (Vec<Envelope>, HashMap<String, MessageBody>, Vec<Label>) {
    let mut envelopes = Vec::new();
    let mut bodies = HashMap::new();
    let now = Utc::now();

    // -- Labels ---------------------------------------------------------------
    let labels = vec![
        make_label(account_id, "Inbox", LabelKind::System, "INBOX"),
        make_label(account_id, "Sent", LabelKind::System, "SENT"),
        make_label(account_id, "Trash", LabelKind::System, "TRASH"),
        make_label(account_id, "Spam", LabelKind::System, "SPAM"),
        make_label(account_id, "Starred", LabelKind::System, "STARRED"),
        make_label(account_id, "Work", LabelKind::User, "work"),
        make_label(account_id, "Personal", LabelKind::User, "personal"),
        make_label(account_id, "Newsletters", LabelKind::User, "newsletters"),
    ];

    let mut msg_num = 1;

    // Thread 1: Deployment plan (4 messages)
    let t1 = ThreadId::new();
    let people = [
        ("Alice Chen", "alice@work.com"),
        ("Bob Kim", "bob@work.com"),
        ("Carol Lee", "carol@work.com"),
        ("Alice Chen", "alice@work.com"),
    ];
    let subjects = [
        "Deployment plan for v2.3",
        "Re: Deployment plan for v2.3",
        "Re: Deployment plan for v2.3",
        "Re: Deployment plan for v2.3",
    ];
    let snippets = [
        "Here's the rollback strategy for v2.3 deployment...",
        "Looks good! One question about the canary percentage...",
        "I'd suggest 5% canary for at least 30 minutes...",
        "Agreed. Let's go with 5% canary. Scheduling for Thursday.",
    ];
    let body_texts = [
        "Here's the rollback strategy for v2.3 deployment.\n\n1. Canary to 5%\n2. Monitor for 30min\n3. Full rollout\n\nRollback trigger: >1% error rate.",
        "Looks good! One question about the canary percentage—should we start at 2% given the auth changes?",
        "I'd suggest 5% canary for at least 30 minutes. The auth changes have been tested extensively in staging.",
        "Agreed. Let's go with 5% canary. Scheduling for Thursday 2pm UTC.",
    ];
    let flags_list = [
        MessageFlags::READ | MessageFlags::STARRED,
        MessageFlags::READ,
        MessageFlags::empty(),
        MessageFlags::empty(),
    ];
    for i in 0..4 {
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t1,
            people[i].0,
            people[i].1,
            subjects[i],
            snippets[i],
            body_texts[i],
            now - Duration::days(2) + Duration::hours(i as i64),
            flags_list[i],
            false,
            UnsubscribeMethod::None,
        );
    }

    // Thread 2: Q1 Report (3 messages)
    let t2 = ThreadId::new();
    for i in 0..3 {
        let (from_name, from_email) = match i {
            0 => ("Diana Park", "diana@work.com"),
            1 => ("Eve Zhang", "eve@work.com"),
            _ => ("Diana Park", "diana@work.com"),
        };
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t2,
            from_name,
            from_email,
            if i == 0 {
                "Q1 Report review"
            } else {
                "Re: Q1 Report review"
            },
            &format!("Q1 report snippet part {}", i + 1),
            &format!("Q1 report body text section {}. Revenue up 12% QoQ.", i + 1),
            now - Duration::days(5) + Duration::hours(i as i64 * 3),
            MessageFlags::READ,
            i == 0, // first msg has attachment
            UnsubscribeMethod::None,
        );
    }

    // Thread 3: Rust newsletter (1 message)
    let t3 = ThreadId::new();
    push_msg(
        &mut envelopes,
        &mut bodies,
        &mut msg_num,
        account_id,
        &t3,
        "This Week in Rust",
        "noreply@rust-lang.org",
        "This Week in Rust #580",
        "Crate of the week: tantivy 0.22 released with new features...",
        "Hello Rustacean! This week's highlights include tantivy 0.22 release.",
        now - Duration::days(1),
        MessageFlags::empty(),
        false,
        UnsubscribeMethod::OneClick {
            url: "https://this-week-in-rust.org/unsubscribe".to_string(),
        },
    );

    // Thread 4: Invoice (2 messages)
    let t4 = ThreadId::new();
    for i in 0..2 {
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t4,
            if i == 0 {
                "Frank Billing"
            } else {
                "Grace Admin"
            },
            if i == 0 {
                "billing@vendor.com"
            } else {
                "grace@work.com"
            },
            if i == 0 {
                "Invoice #2847"
            } else {
                "Re: Invoice #2847"
            },
            if i == 0 {
                "Please find attached invoice #2847..."
            } else {
                "Approved for payment. cc: accounting."
            },
            if i == 0 {
                "Invoice #2847 for consulting services. Amount: $15,000."
            } else {
                "Approved for payment. Please process by EOM."
            },
            now - Duration::days(3) + Duration::hours(i as i64 * 4),
            if i == 0 {
                MessageFlags::READ
            } else {
                MessageFlags::empty()
            },
            i == 0,
            UnsubscribeMethod::None,
        );
    }

    // Thread 5: Team standup (3 messages)
    let t5 = ThreadId::new();
    for i in 0..3 {
        let names = [
            ("Hank Dev", "hank@work.com"),
            ("Iris QA", "iris@work.com"),
            ("Jack PM", "jack@work.com"),
        ];
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t5,
            names[i].0,
            names[i].1,
            if i == 0 {
                "Team standup notes"
            } else {
                "Re: Team standup notes"
            },
            &format!("Standup update from {}", names[i].0),
            &format!("{}: Working on feature X. No blockers.", names[i].0),
            now - Duration::days(1) + Duration::hours(i as i64),
            MessageFlags::READ,
            false,
            UnsubscribeMethod::None,
        );
    }

    // Thread 6: Summer trip (2 messages, personal)
    let t6 = ThreadId::new();
    for i in 0..2 {
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t6,
            if i == 0 { "Kim Travel" } else { "Liam Friend" },
            if i == 0 {
                "kim@personal.com"
            } else {
                "liam@personal.com"
            },
            if i == 0 {
                "Summer trip planning"
            } else {
                "Re: Summer trip planning"
            },
            if i == 0 {
                "Let's plan our summer trip to Japan!"
            } else {
                "Great idea! I've been looking at flights."
            },
            if i == 0 {
                "I've been researching Tokyo hotels and found some great deals for August."
            } else {
                "Flights from SFO are around $800 round trip. Let's book soon!"
            },
            now - Duration::days(4) + Duration::hours(i as i64 * 6),
            if i == 0 {
                MessageFlags::READ
            } else {
                MessageFlags::empty()
            },
            false,
            UnsubscribeMethod::None,
        );
    }

    // Thread 7: PR review (5 messages, work, starred)
    let t7 = ThreadId::new();
    let pr_people = [
        ("Mia Engineer", "mia@work.com"),
        ("Nate Reviewer", "nate@work.com"),
        ("Mia Engineer", "mia@work.com"),
        ("Oscar Lead", "oscar@work.com"),
        ("Mia Engineer", "mia@work.com"),
    ];
    for (i, (name, email)) in pr_people.iter().enumerate() {
        let flags = if i < 3 {
            MessageFlags::READ
        } else {
            MessageFlags::empty()
        } | if i == 0 {
            MessageFlags::STARRED
        } else {
            MessageFlags::empty()
        };
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t7,
            name,
            email,
            if i == 0 {
                "PR review: fix auth middleware"
            } else {
                "Re: PR review: fix auth middleware"
            },
            &format!("PR comment round {}", i + 1),
            &format!(
                "Review comment {}: The auth middleware change looks correct.",
                i + 1
            ),
            now - Duration::days(1) + Duration::hours(i as i64),
            flags,
            false,
            UnsubscribeMethod::None,
        );
    }

    // Thread 8: HN Weekly Digest (1 message)
    let t8 = ThreadId::new();
    push_msg(
        &mut envelopes,
        &mut bodies,
        &mut msg_num,
        account_id,
        &t8,
        "Hacker News",
        "digest@hn.algolia.com",
        "HN Weekly Digest",
        "Top stories this week: Rust in production at scale...",
        "Your weekly Hacker News digest. Top stories include Rust adoption.",
        now - Duration::hours(12),
        MessageFlags::empty(),
        false,
        UnsubscribeMethod::HttpLink {
            url: "https://hn.algolia.com/unsubscribe".to_string(),
        },
    );

    // Thread 9: RustConf invite (2 messages, personal)
    let t9 = ThreadId::new();
    for i in 0..2 {
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t9,
            if i == 0 {
                "RustConf Team"
            } else {
                "Pat Colleague"
            },
            if i == 0 {
                "info@rustconf.com"
            } else {
                "pat@work.com"
            },
            if i == 0 {
                "RustConf 2026 invite"
            } else {
                "Re: RustConf 2026 invite"
            },
            if i == 0 {
                "You're invited to RustConf 2026!"
            } else {
                "Want to go together? Company might sponsor."
            },
            if i == 0 {
                "RustConf 2026 in Portland, Sept 15-17. Early bird tickets available!"
            } else {
                "I asked my manager and they'll cover the ticket. Let's go!"
            },
            now - Duration::days(7) + Duration::hours(i as i64 * 24),
            MessageFlags::empty(),
            i == 0,
            UnsubscribeMethod::None,
        );
    }

    // Thread 10: CI pipeline (3 messages, work)
    let t10 = ThreadId::new();
    for i in 0..3 {
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t10,
            "CI Bot",
            "ci@work.com",
            if i == 0 {
                "CI pipeline failures"
            } else {
                "Re: CI pipeline failures"
            },
            &format!("Pipeline run #{} failed: test timeout", 487 + i),
            &format!(
                "Build #{} failed at stage: integration-tests. Error: timeout after 600s.",
                487 + i
            ),
            now - Duration::hours(6) + Duration::hours(i as i64),
            if i == 0 {
                MessageFlags::READ
            } else {
                MessageFlags::empty()
            },
            false,
            UnsubscribeMethod::None,
        );
    }

    // Thread 11: Changelog newsletter (1 message)
    let t11 = ThreadId::new();
    push_msg(
        &mut envelopes,
        &mut bodies,
        &mut msg_num,
        account_id,
        &t11,
        "The Changelog",
        "noreply@changelog.com",
        "Changelog newsletter #523",
        "This week: SQLite internals deep dive...",
        "Weekly changelog: SQLite internals, new Rust crates, and more.",
        now - Duration::days(3),
        MessageFlags::empty(),
        false,
        UnsubscribeMethod::Mailto {
            address: "unsub@changelog.com".to_string(),
            subject: Some("unsubscribe".to_string()),
        },
    );

    // Thread 12: Fill remaining to reach 55 total
    // Currently: 4+3+1+2+3+2+5+1+2+3+1 = 27 messages
    // Need: 55 - 27 = 28 more
    let filler_subjects = [
        "Weekly sync agenda",
        "Database migration plan",
        "Code review guidelines",
        "Team offsite planning",
        "New hire onboarding",
        "Security audit results",
        "API design discussion",
        "Performance benchmarks",
        "Release notes v2.2",
        "Bug report: login timeout",
        "Feature request: dark mode",
        "Infra cost review",
        "Design system update",
        "Sprint retro notes",
        "Customer feedback summary",
        "Monitoring alert config",
        "Documentation update",
        "Test coverage report",
        "Dependency audit",
        "Architecture decision record",
        "Incident postmortem",
        "License compliance check",
        "Mobile app update",
        "Backend refactoring plan",
        "Data pipeline status",
        "Analytics dashboard",
        "A/B test results",
        "Quarterly planning notes",
    ];
    let filler_from = [
        ("Quinn Dev", "quinn@work.com"),
        ("Rosa PM", "rosa@work.com"),
        ("Sam Ops", "sam@work.com"),
        ("Tina Lead", "tina@work.com"),
        ("Uma Designer", "uma@work.com"),
        ("Vic Security", "vic@work.com"),
        ("Wendy Data", "wendy@work.com"),
    ];

    for i in 0..28 {
        let t = ThreadId::new();
        let (name, email) = filler_from[i % filler_from.len()];
        let flags = if i % 3 == 0 {
            MessageFlags::READ
        } else if i % 7 == 0 {
            MessageFlags::READ | MessageFlags::STARRED
        } else {
            MessageFlags::empty()
        };
        let unsub = if i == 5 {
            UnsubscribeMethod::BodyLink {
                url: "https://example.com/unsub".to_string(),
            }
        } else {
            UnsubscribeMethod::None
        };
        push_msg(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &t,
            name,
            email,
            filler_subjects[i],
            &format!("Preview of: {}", filler_subjects[i]),
            &format!(
                "Full body content for: {}. This contains detailed discussion.",
                filler_subjects[i]
            ),
            now - Duration::days(i as i64 % 30) - Duration::hours(i as i64),
            flags,
            false,
            unsub,
        );
    }

    (envelopes, bodies, labels)
}

fn make_label(account_id: &AccountId, name: &str, kind: LabelKind, provider_id: &str) -> Label {
    Label {
        id: LabelId::new(),
        account_id: account_id.clone(),
        name: name.to_string(),
        kind,
        color: None,
        provider_id: provider_id.to_string(),
        unread_count: 0,
        total_count: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_msg(
    envelopes: &mut Vec<Envelope>,
    bodies: &mut HashMap<String, MessageBody>,
    msg_num: &mut usize,
    account_id: &AccountId,
    thread_id: &ThreadId,
    from_name: &str,
    from_email: &str,
    subject: &str,
    snippet: &str,
    body_text: &str,
    date: chrono::DateTime<chrono::Utc>,
    flags: MessageFlags,
    has_attachments: bool,
    unsubscribe: UnsubscribeMethod,
) {
    let msg_id = MessageId::new();
    let provider_id = format!("fake-msg-{}", msg_num);
    *msg_num += 1;

    let mut attachments = vec![];
    if has_attachments {
        attachments.push(AttachmentMeta {
            id: AttachmentId::new(),
            message_id: msg_id.clone(),
            filename: format!("attachment-{}.pdf", msg_num),
            mime_type: "application/pdf".to_string(),
            size_bytes: 25000,
            local_path: None,
            provider_id: format!("att-{}", msg_num),
        });
    }

    envelopes.push(Envelope {
        id: msg_id.clone(),
        account_id: account_id.clone(),
        provider_id: provider_id.clone(),
        thread_id: thread_id.clone(),
        message_id_header: Some(format!("<msg-{}@fake.mxr>", msg_num)),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some(from_name.to_string()),
            email: from_email.to_string(),
        },
        to: vec![Address {
            name: Some("User".to_string()),
            email: "user@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: subject.to_string(),
        date,
        flags,
        snippet: snippet.to_string(),
        has_attachments,
        size_bytes: body_text.len() as u64 + 500,
        unsubscribe,
        label_provider_ids: {
            let mut labels = vec!["INBOX".to_string()];
            if flags.contains(MessageFlags::SENT) {
                labels.push("SENT".to_string());
            }
            if flags.contains(MessageFlags::STARRED) {
                labels.push("STARRED".to_string());
            }
            if flags.contains(MessageFlags::TRASH) {
                labels.push("TRASH".to_string());
            }
            if flags.contains(MessageFlags::SPAM) {
                labels.push("SPAM".to_string());
            }
            if !flags.contains(MessageFlags::READ) {
                labels.push("UNREAD".to_string());
            }
            labels
        },
    });

    bodies.insert(
        provider_id,
        MessageBody {
            message_id: msg_id,
            text_plain: Some(body_text.to_string()),
            text_html: None,
            attachments,
            fetched_at: chrono::Utc::now(),
        },
    );
}
