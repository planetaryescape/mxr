use chrono::{Duration, Utc};
use mxr_core::id::*;
use mxr_core::types::*;
use std::collections::HashMap;

pub const CURATED_DEMO_MESSAGE_COUNT: usize = 50;
pub const DEFAULT_DEMO_MESSAGE_COUNT: usize = 50_000;
const MAX_DEMO_MESSAGE_COUNT: usize = 200_000;

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
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: Some("Recipient".to_string()),
            email: "recipient@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Conformance test draft".to_string(),
        content: DraftContent::markdown("Hello from conformance test."),
        attachments: vec![],
        inline_assets: Vec::new(),
        inline_calendar_reply: None,
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
            FixtureMessage {
                from_name: people[i].0,
                from_email: people[i].1,
                subject: subjects[i],
                snippet: snippets[i],
                body_text: body_texts[i],
                date: now - Duration::days(2) + Duration::hours(i as i64),
                flags: flags_list[i],
                has_attachments: false,
                unsubscribe: UnsubscribeMethod::None,
            },
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
            FixtureMessage {
                from_name,
                from_email,
                subject: if i == 0 {
                "Q1 Report review"
            } else {
                "Re: Q1 Report review"
            },
                snippet: &format!("Q1 report snippet part {}", i + 1),
                body_text: &format!("Q1 report body text section {}. Revenue up 12% QoQ.", i + 1),
                date: now - Duration::days(5) + Duration::hours(i as i64 * 3),
                flags: MessageFlags::READ,
                has_attachments: i == 0,
                unsubscribe: // first msg has attachment
            UnsubscribeMethod::None,
            },
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
        FixtureMessage {
            from_name: "This Week in Rust",
            from_email: "noreply@rust-lang.org",
            subject: "This Week in Rust #580",
            snippet: "Crate of the week: tantivy 0.22 released with new features...",
            body_text: "Hello Rustacean! This week's highlights include tantivy 0.22 release.",
            date: now - Duration::days(1),
            flags: MessageFlags::empty(),
            has_attachments: false,
            unsubscribe: UnsubscribeMethod::OneClick {
                url: "https://this-week-in-rust.org/unsubscribe".to_string(),
            },
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
            FixtureMessage {
                from_name: if i == 0 {
                    "Frank Billing"
                } else {
                    "Grace Admin"
                },
                from_email: if i == 0 {
                    "billing@vendor.com"
                } else {
                    "grace@work.com"
                },
                subject: if i == 0 {
                    "Invoice #2847"
                } else {
                    "Re: Invoice #2847"
                },
                snippet: if i == 0 {
                    "Please find attached invoice #2847..."
                } else {
                    "Approved for payment. cc: accounting."
                },
                body_text: if i == 0 {
                    "Invoice #2847 for consulting services. Amount: $15,000."
                } else {
                    "Approved for payment. Please process by EOM."
                },
                date: now - Duration::days(3) + Duration::hours(i as i64 * 4),
                flags: if i == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                },
                has_attachments: i == 0,
                unsubscribe: UnsubscribeMethod::None,
            },
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
            FixtureMessage {
                from_name: names[i].0,
                from_email: names[i].1,
                subject: if i == 0 {
                    "Team standup notes"
                } else {
                    "Re: Team standup notes"
                },
                snippet: &format!("Standup update from {}", names[i].0),
                body_text: &format!("{}: Working on feature X. No blockers.", names[i].0),
                date: now - Duration::days(1) + Duration::hours(i as i64),
                flags: MessageFlags::READ,
                has_attachments: false,
                unsubscribe: UnsubscribeMethod::None,
            },
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
            FixtureMessage {
                from_name: if i == 0 { "Kim Travel" } else { "Liam Friend" },
                from_email: if i == 0 {
                    "kim@personal.com"
                } else {
                    "liam@personal.com"
                },
                subject: if i == 0 {
                    "Summer trip planning"
                } else {
                    "Re: Summer trip planning"
                },
                snippet: if i == 0 {
                    "Let's plan our summer trip to Japan!"
                } else {
                    "Great idea! I've been looking at flights."
                },
                body_text: if i == 0 {
                    "I've been researching Tokyo hotels and found some great deals for August."
                } else {
                    "Flights from SFO are around $800 round trip. Let's book soon!"
                },
                date: now - Duration::days(4) + Duration::hours(i as i64 * 6),
                flags: if i == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                },
                has_attachments: false,
                unsubscribe: UnsubscribeMethod::None,
            },
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
            FixtureMessage {
                from_name: name,
                from_email: email,
                subject: if i == 0 {
                    "PR review: fix auth middleware"
                } else {
                    "Re: PR review: fix auth middleware"
                },
                snippet: &format!("PR comment round {}", i + 1),
                body_text: &format!(
                    "Review comment {}: The auth middleware change looks correct.",
                    i + 1
                ),
                date: now - Duration::days(1) + Duration::hours(i as i64),
                flags,
                has_attachments: false,
                unsubscribe: UnsubscribeMethod::None,
            },
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
        FixtureMessage {
            from_name: "Hacker News",
            from_email: "digest@hn.algolia.com",
            subject: "HN Weekly Digest",
            snippet: "Top stories this week: Rust in production at scale...",
            body_text: "Your weekly Hacker News digest. Top stories include Rust adoption.",
            date: now - Duration::hours(12),
            flags: MessageFlags::empty(),
            has_attachments: false,
            unsubscribe: UnsubscribeMethod::HttpLink {
                url: "https://hn.algolia.com/unsubscribe".to_string(),
            },
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
            FixtureMessage {
                from_name: if i == 0 {
                    "RustConf Team"
                } else {
                    "Pat Colleague"
                },
                from_email: if i == 0 {
                    "info@rustconf.com"
                } else {
                    "pat@work.com"
                },
                subject: if i == 0 {
                    "RustConf 2026 invite"
                } else {
                    "Re: RustConf 2026 invite"
                },
                snippet: if i == 0 {
                    "You're invited to RustConf 2026!"
                } else {
                    "Want to go together? Company might sponsor."
                },
                body_text: if i == 0 {
                    "RustConf 2026 in Portland, Sept 15-17. Early bird tickets available!"
                } else {
                    "I asked my manager and they'll cover the ticket. Let's go!"
                },
                date: now - Duration::days(7) + Duration::hours(i as i64 * 24),
                flags: MessageFlags::empty(),
                has_attachments: i == 0,
                unsubscribe: UnsubscribeMethod::None,
            },
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
            FixtureMessage {
                from_name: "CI Bot",
                from_email: "ci@work.com",
                subject: if i == 0 {
                    "CI pipeline failures"
                } else {
                    "Re: CI pipeline failures"
                },
                snippet: &format!("Pipeline run #{} failed: test timeout", 487 + i),
                body_text: &format!(
                    "Build #{} failed at stage: integration-tests. Error: timeout after 600s.",
                    487 + i
                ),
                date: now - Duration::hours(6) + Duration::hours(i as i64),
                flags: if i == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                },
                has_attachments: false,
                unsubscribe: UnsubscribeMethod::None,
            },
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
        FixtureMessage {
            from_name: "The Changelog",
            from_email: "noreply@changelog.com",
            subject: "Changelog newsletter #523",
            snippet: "This week: SQLite internals deep dive...",
            body_text: "Weekly changelog: SQLite internals, new Rust crates, and more.",
            date: now - Duration::days(3),
            flags: MessageFlags::empty(),
            has_attachments: false,
            unsubscribe: UnsubscribeMethod::Mailto {
                address: "unsub@changelog.com".to_string(),
                subject: Some("unsubscribe".to_string()),
            },
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
            FixtureMessage {
                from_name: name,
                from_email: email,
                subject: filler_subjects[i],
                snippet: &format!("Preview of: {}", filler_subjects[i]),
                body_text: &format!(
                    "Full body content for: {}. This contains detailed discussion.",
                    filler_subjects[i]
                ),
                date: now - Duration::days(i as i64 % 30) - Duration::hours(i as i64),
                flags,
                has_attachments: false,
                unsubscribe: unsub,
            },
        );
    }

    (envelopes, bodies, labels)
}

/// Demo message count requested through the environment, or `None` when
/// the provider should use the canonical (non-demo) fixture set.
pub(crate) fn demo_message_count_from_env() -> Option<usize> {
    let dataset = std::env::var("MXR_FAKE_DATASET").ok()?;
    if dataset.trim() != "demo" {
        return None;
    }

    let requested = std::env::var("MXR_FAKE_MESSAGE_COUNT")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_DEMO_MESSAGE_COUNT);
    Some(requested.clamp(1, MAX_DEMO_MESSAGE_COUNT))
}

/// Materialise the whole demo dataset. Only tests want this shape; the
/// provider streams pages instead.
#[cfg(test)]
pub(crate) fn generate_demo_fixtures(
    account_id: &AccountId,
    target_count: usize,
) -> (Vec<Envelope>, HashMap<String, MessageBody>, Vec<Label>) {
    let stream = DemoFixtureStream::new(account_id, target_count);
    let mut envelopes = Vec::with_capacity(stream.len());
    let mut bodies = HashMap::with_capacity(stream.len());
    for (envelope, body) in stream.page(0, stream.len()) {
        bodies.insert(envelope.provider_id.clone(), body);
        envelopes.push(envelope);
    }
    (envelopes, bodies, stream.labels().to_vec())
}

const DEMO_PEOPLE: [(&str, &str); 12] = [
    ("Maya Ortiz", "maya@orbit.example"),
    ("Theo Nash", "theo@northstar.example"),
    ("Priya Raman", "priya@atlas.example"),
    ("Jon Bell", "jon@papertrail.example"),
    ("Nora Kim", "nora@foundry.example"),
    ("Samir Patel", "samir@launchpad.example"),
    ("Elena Wood", "elena@harbor.example"),
    ("Ari Stone", "ari@fieldkit.example"),
    ("Ruth Vega", "ruth@keystone.example"),
    ("Cal Brooks", "cal@signal.example"),
    ("Iris Chen", "iris@meridian.example"),
    ("Leo Park", "leo@workbench.example"),
];
const DEMO_NEWSLETTER_SENDERS: [(&str, &str); 4] = [
    ("Terminal Dispatch", "dispatch@lists.demo.mxr.local"),
    ("Local First Weekly", "weekly@lists.demo.mxr.local"),
    ("SQLite Notes", "notes@lists.demo.mxr.local"),
    ("Ops Digest", "ops@lists.demo.mxr.local"),
];
const DEMO_ALERT_SENDERS: [(&str, &str); 3] = [
    ("Build Watch", "builds@alerts.demo.mxr.local"),
    ("Uptime Robot", "uptime@alerts.demo.mxr.local"),
    ("Pager Relay", "pager@alerts.demo.mxr.local"),
];
const DEMO_PROMO_SENDERS: [(&str, &str); 4] = [
    ("Cloud Deals", "offers@promo.demo.mxr.local"),
    ("Desk Gear Outlet", "gear@promo.demo.mxr.local"),
    ("Flight Fare Drop", "fares@promo.demo.mxr.local"),
    ("Conference Passes", "events@promo.demo.mxr.local"),
];
const DEMO_SPAM_SENDERS: [(&str, &str); 3] = [
    (
        "Account Verification",
        "verify@security-mail.demo.mxr.local",
    ),
    ("Prize Desk", "winner@claim-now.demo.mxr.local"),
    ("Payroll Notice", "payroll@notice-demo.mxr.local"),
];
const DEMO_SUBJECT_TEMPLATES: [&str; 20] = [
    "Launch checklist for Project Aurora",
    "Canary rollout notes",
    "Customer feedback from the design partner call",
    "Draft positioning for the CLI-first release",
    "Security review follow-up",
    "QBR prep and open questions",
    "Contract renewal details",
    "Interview panel for Staff Engineer candidate",
    "Receipt for workspace subscription",
    "Flight options for the Portland demo day",
    "Incident review: delayed sync jobs",
    "Weekly local-first reading list",
    "Build failed on release branch",
    "Pricing page copy review",
    "API migration plan",
    "Research notes: terminal workflows",
    "Action required: unusual sign-in attempt",
    "Spring upgrade offer for your workspace",
    "Limited-time launch bundle",
    "Urgent password reset notice",
];

/// Number of messages `delivery_demo_messages` injects at the head of the
/// personal demo account.
const DELIVERY_DEMO_MESSAGE_COUNT: usize = 5;

/// The demo dataset, generated on demand instead of held in memory.
///
/// A 50,000-message demo used to cost ~2.5 KB of resident memory per
/// message for the whole life of the daemon, because the provider
/// materialised every envelope and body up front. Generation is pure
/// arithmetic and `format!` over `(thread_num, reply_idx, index)`, so a
/// page can be produced from its index alone and nothing needs to be
/// kept: `page()` walks thread lengths (cheap integer math) to reach the
/// requested offset and only builds the messages it returns.
///
/// Ids are derived from the message's provider id rather than randomly
/// generated so regenerating the same index — a later page, a
/// `fetch_message` — yields the same message.
pub(crate) struct DemoFixtureStream {
    account_id: AccountId,
    profile: DemoAccountProfile,
    labels: Vec<Label>,
    /// Pinned once so every page shares one "now" and message dates stay
    /// consistent across pages of the same sync.
    now: chrono::DateTime<Utc>,
    self_addr: Address,
    delivery_count: usize,
    /// The 50-message dataset is hand-written rather than generated, so
    /// it stays materialised. It is small by definition.
    curated: Option<Vec<(Envelope, MessageBody)>>,
}

impl DemoFixtureStream {
    pub fn new(account_id: &AccountId, target_count: usize) -> Self {
        let target_count = target_count.clamp(1, MAX_DEMO_MESSAGE_COUNT);
        let profile = demo_account_profile(account_id, target_count);
        let now = Utc::now();
        let self_addr = Address {
            name: Some(profile.display_name.to_string()),
            email: profile.email.to_string(),
        };
        let labels = demo_labels(account_id);

        let curated = (target_count == CURATED_DEMO_MESSAGE_COUNT).then(|| {
            let (envelopes, mut bodies, _) =
                generate_curated_demo_fixtures(account_id, profile, labels.clone());
            envelopes
                .into_iter()
                .map(|envelope| {
                    let body = bodies
                        .remove(&envelope.provider_id)
                        .unwrap_or_else(|| empty_body(&envelope.id));
                    (envelope, body)
                })
                .collect()
        });

        // Seed the personal account with shipping mail so the Deliveries
        // surface is populated. These occupy the first indices; the
        // generated threads top up to `target_count`, so the total
        // message count is unchanged.
        let delivery_count = if profile.email == "alex@demo.mxr.local" && profile.target_count >= 16
        {
            DELIVERY_DEMO_MESSAGE_COUNT
        } else {
            0
        };

        Self {
            account_id: account_id.clone(),
            profile,
            labels,
            now,
            self_addr,
            delivery_count,
            curated,
        }
    }

    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    pub fn len(&self) -> usize {
        match &self.curated {
            Some(curated) => curated.len(),
            None => self.profile.target_count,
        }
    }

    /// Generate `[offset, offset + limit)` of the dataset.
    pub fn page(&self, offset: usize, limit: usize) -> Vec<(Envelope, MessageBody)> {
        let end = offset.saturating_add(limit).min(self.len());
        if offset >= end {
            return Vec::new();
        }
        if let Some(curated) = &self.curated {
            return curated[offset..end].to_vec();
        }

        let mut page = Vec::with_capacity(end - offset);
        if offset < self.delivery_count {
            page.extend(
                self.delivery_messages()
                    .into_iter()
                    .skip(offset)
                    .take(end - offset),
            );
        }

        let mut produced = self.delivery_count;
        let mut thread_num = 0usize;
        while produced < self.profile.target_count && produced < end {
            // `thread_len` drives dates and the "last message in thread"
            // rules, so the message builder always sees the thread's full
            // length even when the target count truncates the tail.
            let thread_len = demo_thread_len(thread_num);
            let emitted = thread_len.min(self.profile.target_count - produced);
            for reply_idx in 0..emitted {
                let index = produced + reply_idx;
                if index >= offset && index < end {
                    page.push(self.thread_message(thread_num, reply_idx, thread_len, index));
                }
            }
            produced += emitted;
            thread_num += 1;
        }
        page
    }

    /// Look up one message by its provider id, regenerating it on the fly.
    /// Walks thread lengths to reach the message, so it is a scan over
    /// integers rather than over messages.
    pub fn find(&self, provider_id: &str) -> Option<(Envelope, MessageBody)> {
        if let Some(curated) = &self.curated {
            return curated
                .iter()
                .find(|(envelope, _)| envelope.provider_id == provider_id)
                .cloned();
        }
        let index = provider_id
            .strip_prefix("demo-msg-")?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)?;
        self.page(index, 1).pop()
    }

    fn delivery_messages(&self) -> Vec<(Envelope, MessageBody)> {
        delivery_demo_messages(&self.account_id, &self.self_addr, self.now)
    }

    fn thread_message(
        &self,
        thread_num: usize,
        reply_idx: usize,
        thread_len: usize,
        index: usize,
    ) -> (Envelope, MessageBody) {
        let category = thread_num % 13;
        let root_subject = DEMO_SUBJECT_TEMPLATES[thread_num % DEMO_SUBJECT_TEMPLATES.len()];
        let thread_offset_minutes = (thread_num as i64 * 37) % (365 * 24 * 60);

        let sent = matches!(category, 0 | 1 | 5) && reply_idx % 3 == 1;
        let (name, email) = match category {
            2 => DEMO_NEWSLETTER_SENDERS[thread_num % DEMO_NEWSLETTER_SENDERS.len()],
            7 => DEMO_ALERT_SENDERS[thread_num % DEMO_ALERT_SENDERS.len()],
            10 | 12 => DEMO_SPAM_SENDERS[thread_num % DEMO_SPAM_SENDERS.len()],
            11 => DEMO_PROMO_SENDERS[thread_num % DEMO_PROMO_SENDERS.len()],
            _ => DEMO_PEOPLE[(thread_num + reply_idx) % DEMO_PEOPLE.len()],
        };
        let peer = Address {
            name: Some(name.to_string()),
            email: email.to_string(),
        };
        let observer = DEMO_PEOPLE[(thread_num + reply_idx + 3) % DEMO_PEOPLE.len()];
        let observer = Address {
            name: Some(observer.0.to_string()),
            email: observer.1.to_string(),
        };
        let cc = if matches!(category, 0 | 3 | 4 | 8) && observer.email != peer.email {
            vec![observer]
        } else {
            Vec::new()
        };
        let (from, to) = if sent {
            (self.self_addr.clone(), vec![peer.clone()])
        } else {
            (peer.clone(), vec![self.self_addr.clone()])
        };

        let subject = if reply_idx == 0 {
            root_subject.to_string()
        } else {
            format!("Re: {root_subject}")
        };
        let date = self.now
            - Duration::minutes(thread_offset_minutes + (thread_len - reply_idx) as i64 * 47);
        let old_enough_to_read = thread_offset_minutes > 24 * 60;
        let unread = !sent && !old_enough_to_read && reply_idx + 1 == thread_len;
        let starred = thread_num.is_multiple_of(29) || root_subject.contains("Security");
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
        if category == 10 {
            flags |= MessageFlags::SPAM;
        }
        if category == 9 && thread_num.is_multiple_of(11) {
            flags |= MessageFlags::ARCHIVED;
        }

        let mut provider_labels = Vec::new();
        if sent {
            provider_labels.push("SENT".to_string());
        } else if flags.contains(MessageFlags::SPAM) {
            provider_labels.push("SPAM".to_string());
        } else if flags.contains(MessageFlags::ARCHIVED) {
            provider_labels.push("ARCHIVE".to_string());
        } else {
            provider_labels.push("INBOX".to_string());
        }
        if unread {
            provider_labels.push("UNREAD".to_string());
        }
        if starred {
            provider_labels.push("STARRED".to_string());
        }
        match category {
            0 | 1 | 3 | 9 => provider_labels.push("work".to_string()),
            4 => provider_labels.push("product".to_string()),
            2 => provider_labels.push("newsletters".to_string()),
            5 => provider_labels.push("receipts".to_string()),
            6 => provider_labels.push("travel".to_string()),
            7 => provider_labels.push("alerts".to_string()),
            8 => provider_labels.push("hiring".to_string()),
            10 => provider_labels.push("potential_spam".to_string()),
            11 => provider_labels.push("promotions".to_string()),
            12 => provider_labels.push("potential_spam".to_string()),
            _ => {}
        }
        if matches!(category, 0 | 3 | 8) && !sent && reply_idx + 1 == thread_len {
            provider_labels.push("waiting".to_string());
        }

        let unsubscribe = match category {
            2 if thread_num.is_multiple_of(2) => UnsubscribeMethod::OneClick {
                url: format!("https://lists.demo.mxr.local/unsubscribe/{thread_num}"),
            },
            2 => UnsubscribeMethod::Mailto {
                address: "unsubscribe@lists.demo.mxr.local".to_string(),
                subject: Some(format!("unsubscribe-{thread_num}")),
            },
            11 => UnsubscribeMethod::OneClick {
                url: format!("https://promo.demo.mxr.local/unsubscribe/{thread_num}"),
            },
            _ => UnsubscribeMethod::None,
        };
        let has_attachments = matches!(category, 5 | 6 | 8)
            || (matches!(category, 2 | 11) && thread_num.is_multiple_of(8))
            || thread_num.is_multiple_of(41);
        let snippet = demo_snippet(root_subject, category, reply_idx, sent);
        let body_text = demo_body(root_subject, category, reply_idx, sent, &peer.email);

        // Messages of a thread occupy consecutive indices, so the parent
        // and root headers follow from this message's own index.
        let (in_reply_to, references) = if reply_idx == 0 {
            (None, Vec::new())
        } else {
            (
                Some(demo_message_id_header(index)),
                vec![demo_message_id_header(index - reply_idx + 1)],
            )
        };

        build_demo_msg(
            index + 1,
            &self.account_id,
            &demo_thread_id(&self.account_id, thread_num),
            DemoMessage {
                from,
                to,
                cc,
                subject,
                snippet,
                body_text,
                date,
                flags,
                has_attachments,
                category,
                unsubscribe,
                label_provider_ids: provider_labels,
                in_reply_to,
                references,
            },
        )
    }
}

fn demo_labels(account_id: &AccountId) -> Vec<Label> {
    vec![
        make_label(account_id, "Inbox", LabelKind::System, "INBOX"),
        make_label(account_id, "Sent", LabelKind::System, "SENT"),
        make_label(account_id, "Trash", LabelKind::System, "TRASH"),
        make_label(account_id, "Spam", LabelKind::System, "SPAM"),
        make_label(account_id, "Starred", LabelKind::System, "STARRED"),
        make_label(account_id, "Work", LabelKind::User, "work"),
        make_label(account_id, "Product", LabelKind::User, "product"),
        make_label(account_id, "Newsletters", LabelKind::User, "newsletters"),
        make_label(account_id, "Receipts", LabelKind::User, "receipts"),
        make_label(account_id, "Travel", LabelKind::User, "travel"),
        make_label(account_id, "Alerts", LabelKind::User, "alerts"),
        make_label(account_id, "Hiring", LabelKind::User, "hiring"),
        make_label(account_id, "Waiting", LabelKind::User, "waiting"),
        make_label(account_id, "Promotions", LabelKind::User, "promotions"),
        make_label(
            account_id,
            "Potential Spam",
            LabelKind::User,
            "potential_spam",
        ),
    ]
}

fn demo_thread_len(thread_num: usize) -> usize {
    match thread_num % 13 {
        1 | 4 => 5,
        2 | 7 | 10 | 11 | 12 => 1,
        8 => 3,
        _ => 2 + (thread_num % 4),
    }
}

fn demo_thread_id(account_id: &AccountId, thread_num: usize) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-thread-{thread_num}"))
}

fn demo_message_id_header(msg_num: usize) -> String {
    format!("<demo-{msg_num}@mxr.local>")
}

/// Placeholder for a message whose body is missing. Fixture bodies are
/// always generated alongside their envelope, so this only guards the
/// lookup, it is not a normal state.
pub(crate) fn empty_body(message_id: &MessageId) -> MessageBody {
    MessageBody {
        message_id: message_id.clone(),
        text_plain: None,
        text_html: None,
        attachments: vec![],
        fetched_at: Utc::now(),
        metadata: Default::default(),
    }
}

fn generate_curated_demo_fixtures(
    account_id: &AccountId,
    profile: DemoAccountProfile,
    labels: Vec<Label>,
) -> (Vec<Envelope>, HashMap<String, MessageBody>, Vec<Label>) {
    let mut envelopes = Vec::with_capacity(profile.target_count);
    let mut bodies = HashMap::with_capacity(profile.target_count);
    let now = Utc::now();
    let self_addr = Address {
        name: Some(profile.display_name.to_string()),
        email: profile.email.to_string(),
    };
    let senders = curated_demo_senders();
    let subjects = [
        "Launch checklist for Project Aurora",
        "Canary rollout notes",
        "Design partner feedback",
        "CLI-first release positioning",
        "Security review follow-up",
        "QBR prep and open questions",
        "Receipt for workspace subscription",
        "Portland demo day travel",
        "Build failed on release branch",
        "Local-first weekly reading list",
        "Spring upgrade offer",
        "Action required: unusual sign-in attempt",
    ];
    let thread_lengths = [3usize, 2, 1, 4, 2, 3, 1, 2, 1, 1, 1, 1];

    let mut msg_num = 1usize;
    let mut thread_num = 0usize;
    // Personal account gets shipping mail so the curated showcase includes the
    // Deliveries surface. Injected before the filler loop tops up to
    // `target_count`, keeping the curated count stable.
    if profile.email == "alex@demo.mxr.local" && profile.target_count >= 16 {
        push_delivery_demo_threads(
            &mut envelopes,
            &mut bodies,
            &mut msg_num,
            account_id,
            &self_addr,
            now,
        );
    }
    while envelopes.len() < profile.target_count {
        let sender = senders[thread_num % senders.len()];
        let root_subject = subjects[thread_num % subjects.len()];
        let thread_len = thread_lengths[thread_num % thread_lengths.len()];
        let thread_id = ThreadId::new();
        let mut references = Vec::new();
        let mut previous_message_id = None;

        for reply_idx in 0..thread_len {
            if envelopes.len() >= profile.target_count {
                break;
            }

            let sent = sender.allow_sent_replies && reply_idx == 1;
            let peer = Address {
                name: Some(sender.name.to_string()),
                email: sender.email.to_string(),
            };
            let (from, to) = if sent {
                (self_addr.clone(), vec![peer.clone()])
            } else {
                (peer.clone(), vec![self_addr.clone()])
            };
            let subject = if reply_idx == 0 {
                root_subject.to_string()
            } else {
                format!("Re: {root_subject}")
            };
            let unread = !sent && reply_idx + 1 == thread_len && thread_num.is_multiple_of(3);
            let mut flags = if unread {
                MessageFlags::empty()
            } else {
                MessageFlags::READ
            };
            if sent {
                flags |= MessageFlags::SENT | MessageFlags::READ;
            }
            if sender.starred {
                flags |= MessageFlags::STARRED;
            }
            if sender.provider_label == "SPAM" {
                flags |= MessageFlags::SPAM;
            }

            let mut provider_labels = vec![sender.provider_label.to_string()];
            provider_labels.push(sender.user_label.to_string());
            if unread {
                provider_labels.push("UNREAD".to_string());
            }
            if sender.starred {
                provider_labels.push("STARRED".to_string());
            }

            let unsubscribe = match sender.user_label {
                "newsletters" => UnsubscribeMethod::OneClick {
                    url: format!("https://lists.demo.mxr.local/unsubscribe/{thread_num}"),
                },
                "promotions" => UnsubscribeMethod::Mailto {
                    address: "unsubscribe@promo.demo.mxr.local".to_string(),
                    subject: Some("unsubscribe".to_string()),
                },
                _ => UnsubscribeMethod::None,
            };
            let has_attachments = matches!(sender.user_label, "receipts" | "travel" | "hiring");
            let snippet = format!(
                "{} in curated demo thread: {root_subject}",
                if sent { "You replied" } else { "New update" }
            );
            let body = demo_body(root_subject, sender.category, reply_idx, sent, sender.email);
            let current_header = push_demo_msg(
                &mut envelopes,
                &mut bodies,
                &mut msg_num,
                account_id,
                &thread_id,
                DemoMessage {
                    from,
                    to,
                    cc: Vec::new(),
                    subject,
                    snippet,
                    body_text: body,
                    date: now - Duration::hours((thread_num * 5 + reply_idx) as i64),
                    flags,
                    has_attachments,
                    category: sender.category,
                    unsubscribe,
                    label_provider_ids: provider_labels,
                    in_reply_to: previous_message_id.clone(),
                    references: references.clone(),
                },
            );
            if previous_message_id.is_none() {
                references.push(current_header.clone());
            }
            previous_message_id = Some(current_header);
        }
        thread_num += 1;
    }

    (envelopes, bodies, labels)
}

#[derive(Debug, Clone, Copy)]
struct CuratedDemoSender {
    name: &'static str,
    email: &'static str,
    user_label: &'static str,
    provider_label: &'static str,
    category: usize,
    allow_sent_replies: bool,
    starred: bool,
}

fn curated_demo_senders() -> [CuratedDemoSender; 12] {
    [
        CuratedDemoSender {
            name: "Maya Ortiz",
            email: "maya@orbit.example",
            user_label: "work",
            provider_label: "INBOX",
            category: 0,
            allow_sent_replies: true,
            starred: true,
        },
        CuratedDemoSender {
            name: "Theo Nash",
            email: "theo@northstar.example",
            user_label: "work",
            provider_label: "INBOX",
            category: 1,
            allow_sent_replies: true,
            starred: false,
        },
        CuratedDemoSender {
            name: "Priya Raman",
            email: "priya@atlas.example",
            user_label: "product",
            provider_label: "INBOX",
            category: 4,
            allow_sent_replies: true,
            starred: false,
        },
        CuratedDemoSender {
            name: "Jon Bell",
            email: "jon@papertrail.example",
            user_label: "work",
            provider_label: "INBOX",
            category: 3,
            allow_sent_replies: true,
            starred: false,
        },
        CuratedDemoSender {
            name: "Nora Kim",
            email: "nora@foundry.example",
            user_label: "hiring",
            provider_label: "INBOX",
            category: 8,
            allow_sent_replies: false,
            starred: false,
        },
        CuratedDemoSender {
            name: "Samir Patel",
            email: "samir@launchpad.example",
            user_label: "receipts",
            provider_label: "INBOX",
            category: 5,
            allow_sent_replies: true,
            starred: false,
        },
        CuratedDemoSender {
            name: "Elena Wood",
            email: "elena@harbor.example",
            user_label: "travel",
            provider_label: "INBOX",
            category: 6,
            allow_sent_replies: false,
            starred: false,
        },
        CuratedDemoSender {
            name: "Ari Stone",
            email: "ari@fieldkit.example",
            user_label: "work",
            provider_label: "ARCHIVE",
            category: 9,
            allow_sent_replies: true,
            starred: false,
        },
        CuratedDemoSender {
            name: "Build Watch",
            email: "builds@alerts.demo.mxr.local",
            user_label: "alerts",
            provider_label: "INBOX",
            category: 7,
            allow_sent_replies: false,
            starred: false,
        },
        CuratedDemoSender {
            name: "Terminal Dispatch",
            email: "dispatch@lists.demo.mxr.local",
            user_label: "newsletters",
            provider_label: "INBOX",
            category: 2,
            allow_sent_replies: false,
            starred: false,
        },
        CuratedDemoSender {
            name: "Cloud Deals",
            email: "offers@promo.demo.mxr.local",
            user_label: "promotions",
            provider_label: "INBOX",
            category: 11,
            allow_sent_replies: false,
            starred: false,
        },
        CuratedDemoSender {
            name: "Account Verification",
            email: "verify@security-mail.demo.mxr.local",
            user_label: "potential_spam",
            provider_label: "SPAM",
            category: 10,
            allow_sent_replies: false,
            starred: false,
        },
    ]
}

#[derive(Clone, Copy)]
struct DemoAccountProfile {
    display_name: &'static str,
    email: &'static str,
    target_count: usize,
}

fn demo_account_profile(account_id: &AccountId, total_target: usize) -> DemoAccountProfile {
    let work_id = AccountId::from_provider_id("fake", "alex@work.demo.mxr.local");
    let personal_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
    let work_count = ((total_target as f32) * 0.45).round() as usize;
    if *account_id == work_id {
        DemoAccountProfile {
            display_name: "Alex Work",
            email: "alex@work.demo.mxr.local",
            target_count: work_count.max(1),
        }
    } else if *account_id == personal_id {
        DemoAccountProfile {
            display_name: "Alex Demo",
            email: "alex@demo.mxr.local",
            target_count: total_target.saturating_sub(work_count).max(1),
        }
    } else {
        DemoAccountProfile {
            display_name: "Alex Demo",
            email: "alex@demo.mxr.local",
            target_count: total_target,
        }
    }
}

fn demo_snippet(subject: &str, category: usize, reply_idx: usize, sent: bool) -> String {
    let action = if sent { "You replied" } else { "New update" };
    match category {
        2 => format!("{subject}: links, notes, and unsubscribe metadata"),
        5 => format!("Receipt and invoice details for {subject}"),
        7 => format!("Alert #{reply_idx}: status changed, investigate if this is still active"),
        10 => format!("{subject}: quarantined as spam in the demo dataset"),
        11 => format!("{subject}: promotional offer with unsubscribe metadata"),
        12 => format!("{subject}: still in inbox, but suspicious enough to flag"),
        _ => format!("{action} in thread: {subject}"),
    }
}

fn demo_body(
    subject: &str,
    category: usize,
    reply_idx: usize,
    sent: bool,
    peer_email: &str,
) -> String {
    let perspective = if sent {
        "I tightened the next step and left a clear owner."
    } else {
        "Can you take a look and reply with the next concrete step?"
    };
    let detail = match category {
        0 => "Rollout risk: watch sync latency, auth failures, and support tickets for the first hour. Decision: keep the canary at 5% until Maya confirms the dashboard is quiet. Link: https://demo.mxr.local/runbooks/aurora-rollout",
        1 => "The review thread has enough detail to test search, replies, archive, and labels. Open question: whether the migration should batch 2k or 5k rows at a time.",
        2 => "Newsletter section: SQLite, terminal workflows, local-first apps, and unsubscribe links. Top links: https://demo.mxr.local/articles/local-first-mail and https://sqlite.org/changes.html",
        3 => "Follow-up needed: customer context, owner, deadline, and decision history are all in one thread. Ruth owes the final customer-facing note by Friday.",
        4 => "Product notes: copy, onboarding friction, time-to-wow, and first-run experience. The screenshot in the HTML body is intentionally remote so the remote-images toggle has something to control.",
        5 => "Finance note: receipt attached, amount approved, and billing period included below. Amount: $1,482.17. Cost center: ENG-DEMO.",
        6 => "Travel note: itinerary attached, hotel confirmation, and calendar details. Booking reference: MXR-PORTLAND-42.",
        7 => "Alert details: service recovered, but keep this searchable for the incident review. The first alert fired at 02:14 and recovery was confirmed at 02:31.",
        8 => "Hiring notes: interview plan, scorecard, and candidate follow-up. Candidate strength: pragmatic systems debugging; concern: limited product collaboration examples.",
        10 => "Spam sample: this message is already in Spam. It asks Alex to claim a prize using a suspicious link and includes mismatched sender identity, pressure language, and a throwaway reply address.",
        11 => "Promotion sample: legitimate marketing mail with discount language, tracking links, remote images, and one-click unsubscribe metadata. Good for demoing promo filtering without treating it as spam.",
        12 => "Potential spam sample: this is deliberately still in Inbox. It mentions urgent password reset, unusual sign-in, and action required language so rules and LLM summaries can flag risk without hiding the mail.",
        _ => "General work thread with enough text to make full-body search feel useful.",
    };
    format!(
        "Subject: {subject}\n\nMessage {reply_idx} with {peer_email}. {perspective}\n\n{detail}\n\nSummary hint: capture who asked for the work, who owns the next step, the date or amount if present, and whether Alex already replied.\n\nDemo data is synthetic. It is designed to exercise mxr search, labels, threads, attachments, newsletters, saved searches, reply-later, sender profiles, LLM summaries, and analytics without touching a real inbox."
    )
}

struct DemoMessage {
    from: Address,
    to: Vec<Address>,
    cc: Vec<Address>,
    subject: String,
    snippet: String,
    body_text: String,
    date: chrono::DateTime<chrono::Utc>,
    flags: MessageFlags,
    has_attachments: bool,
    category: usize,
    unsubscribe: UnsubscribeMethod,
    label_provider_ids: Vec<String>,
    in_reply_to: Option<String>,
    references: Vec<String>,
}

/// Inject shipping/delivery mail so the demo profile populates the Deliveries
/// surface. Senders are real carrier/merchant domains and the bodies carry
/// checksum-valid tracking numbers, so the local detection heuristic creates
/// deliveries without needing the LLM. Yields three tracked packages:
///
/// * an Amazon order whose confirmation → shipped → delivered emails collapse
///   into a single delivery that resolves to `delivered` (lifecycle demo);
/// * a UPS package `in_transit`;
/// * a USPS package `out_for_delivery`.
///
/// Tracking numbers are valid fixtures from `tracking_number_data` so the
/// `tracking-numbers` crate accepts them. Bodies deliberately avoid
/// review/survey phrasing ("rate your delivery") which the detector treats as
/// post-delivery noise.
fn delivery_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(Envelope, MessageBody)> {
    let mut messages = Vec::with_capacity(DELIVERY_DEMO_MESSAGE_COUNT);

    // --- Amazon: full lifecycle in one thread → resolves to delivered. ---
    let order = "112-7480913-6624530";
    let tracking = "TBA619632698000"; // Amazon Logistics (valid format)
    let amazon = Address {
        name: Some("Amazon.com".to_string()),
        email: "shipment-tracking@amazon.com".to_string(),
    };
    let thread = delivery_thread_id(account_id, "amazon");
    let lifecycle: [(i64, String, String, MessageFlags); 3] = [
        (
            6,
            format!("Your Amazon.com order #{order} of \"USB-C Cable (2-pack)\""),
            format!(
                "Hello Alex,\n\nThank you for your order. We'll let you know when it ships.\n\nOrder #{order}\nItem: Anker USB-C Cable (2-pack)\nTracking number: {tracking}\nEstimated delivery: in a few days\n\nView your order in Your Orders."
            ),
            MessageFlags::READ,
        ),
        (
            3,
            "Shipped: your Amazon.com order is on the way".to_string(),
            format!(
                "Hello Alex,\n\nYour package has shipped and is on its way.\n\nOrder #{order}\nCarrier: Amazon Logistics\nTracking number: {tracking}\nTrack your package: https://track.amazon.com/?trackingId={tracking}\n\nArriving soon."
            ),
            MessageFlags::READ,
        ),
        (
            1,
            "Delivered: your Amazon.com package".to_string(),
            format!(
                "Hello Alex,\n\nYour package was delivered.\n\nOrder #{order}\nTracking number: {tracking}\nDelivered to: front door\n\nThank you for shopping with us."
            ),
            MessageFlags::empty(),
        ),
    ];
    for (reply_idx, (days_ago, subject, body, flags)) in lifecycle.into_iter().enumerate() {
        let msg_num = reply_idx + 1;
        let (in_reply_to, references) = if reply_idx == 0 {
            (None, Vec::new())
        } else {
            (
                Some(demo_message_id_header(msg_num - 1)),
                vec![demo_message_id_header(1)],
            )
        };
        messages.push(build_demo_msg(
            msg_num,
            account_id,
            &thread,
            DemoMessage {
                from: amazon.clone(),
                to: vec![self_addr.clone()],
                cc: Vec::new(),
                subject,
                snippet: format!("Amazon order #{order} — tracking {tracking}"),
                body_text: body,
                date: now - Duration::days(days_ago),
                flags,
                has_attachments: false,
                category: 5,
                unsubscribe: UnsubscribeMethod::None,
                label_provider_ids: vec!["INBOX".to_string()],
                in_reply_to,
                references,
            },
        ));
    }

    // --- UPS: a single in-transit shipment from a boutique merchant. ---
    messages.push(build_demo_msg(
        4,
        account_id,
        &delivery_thread_id(account_id, "ups"),
        DemoMessage {
            from: Address {
                name: Some("UPS".to_string()),
                email: "mcinfo@ups.com".to_string(),
            },
            to: vec![self_addr.clone()],
            cc: Vec::new(),
            subject: "UPS Update: your Cedar & Sage package is on the way".to_string(),
            snippet: "UPS 1Z5R89390357567127 — arriving Friday".to_string(),
            body_text:
                "Hello Alex,\n\nYour order from Cedar & Sage has shipped and is on the way.\n\nMerchant: Cedar & Sage\nTracking number: 1Z5R89390357567127\nScheduled delivery: Friday by end of day\nTrack: https://www.ups.com/track?tracknum=1Z5R89390357567127"
                    .to_string(),
            date: now - Duration::days(2),
            flags: MessageFlags::empty(),
            has_attachments: false,
            category: 6,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec!["INBOX".to_string()],
            in_reply_to: None,
            references: Vec::new(),
        },
    ));

    // --- USPS: a single out-for-delivery shipment. ---
    messages.push(build_demo_msg(
        5,
        account_id,
        &delivery_thread_id(account_id, "usps"),
        DemoMessage {
            from: Address {
                name: Some("USPS Informed Delivery".to_string()),
                email: "auto-reply@usps.com".to_string(),
            },
            to: vec![self_addr.clone()],
            cc: Vec::new(),
            subject: "Your USPS package is out for delivery".to_string(),
            snippet: "USPS 9400111899560438600329 — out for delivery today".to_string(),
            body_text:
                "Hello Alex,\n\nYour package is out for delivery and should arrive today.\n\nTracking number: 9400111899560438600329\nStatus: Out for Delivery\nTrack: https://tools.usps.com/go/TrackConfirmAction?tLabels=9400111899560438600329"
                    .to_string(),
            date: now - Duration::hours(5),
            flags: MessageFlags::empty(),
            has_attachments: false,
            category: 6,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec!["INBOX".to_string()],
            in_reply_to: None,
            references: Vec::new(),
        },
    ));

    messages
}

/// Materialising variant used by the curated (hand-written) demo dataset,
/// which builds its messages eagerly.
fn push_delivery_demo_threads(
    envelopes: &mut Vec<Envelope>,
    bodies: &mut HashMap<String, MessageBody>,
    msg_num: &mut usize,
    account_id: &AccountId,
    self_addr: &Address,
    now: chrono::DateTime<chrono::Utc>,
) {
    for (envelope, body) in delivery_demo_messages(account_id, self_addr, now) {
        bodies.insert(envelope.provider_id.clone(), body);
        envelopes.push(envelope);
        *msg_num += 1;
    }
}

/// Materialising variant of [`build_demo_msg`] for the curated dataset.
fn push_demo_msg(
    envelopes: &mut Vec<Envelope>,
    bodies: &mut HashMap<String, MessageBody>,
    msg_num: &mut usize,
    account_id: &AccountId,
    thread_id: &ThreadId,
    message: DemoMessage,
) -> String {
    let header = demo_message_id_header(*msg_num);
    let (envelope, body) = build_demo_msg(*msg_num, account_id, thread_id, message);
    *msg_num += 1;
    bodies.insert(envelope.provider_id.clone(), body);
    envelopes.push(envelope);
    header
}

fn delivery_thread_id(account_id: &AccountId, carrier: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-delivery-{carrier}"))
}

fn build_demo_msg(
    msg_num: usize,
    account_id: &AccountId,
    thread_id: &ThreadId,
    message: DemoMessage,
) -> (Envelope, MessageBody) {
    let DemoMessage {
        from,
        to,
        cc,
        subject,
        snippet,
        body_text,
        date,
        flags,
        has_attachments,
        category,
        unsubscribe,
        label_provider_ids,
        in_reply_to,
        references,
    } = message;
    let current_num = msg_num;
    let provider_id = format!("demo-msg-{current_num}");
    // Derived, not random: the same index must regenerate the same message
    // so a later page or a `fetch_message` sees identical ids.
    let msg_id = MessageId::from_scoped_provider_id(account_id, "fake", &provider_id);
    let message_id_header = demo_message_id_header(current_num);

    let attachments = demo_attachments(&msg_id, current_num, category, flags, has_attachments);
    let text_html = demo_html_body(&subject, &body_text, category, current_num);

    let size_bytes = body_text.len() as u64
        + attachments
            .iter()
            .map(|attachment| attachment.size_bytes)
            .sum::<u64>()
        + 700;
    let envelope = Envelope {
        id: msg_id.clone(),
        account_id: account_id.clone(),
        provider_id,
        thread_id: thread_id.clone(),
        message_id_header: Some(message_id_header),
        in_reply_to,
        references,
        from,
        to,
        cc,
        bcc: Vec::new(),
        subject,
        date,
        flags,
        snippet,
        has_attachments: !attachments.is_empty(),
        size_bytes,
        unsubscribe,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids,
        // Seed one fixture with a sample keyword so the keyword
        // round-trip is exercised by the conformance test path.
        keywords: std::collections::BTreeSet::from(["$Forwarded".to_string()]),
    };

    let body = MessageBody {
        message_id: msg_id,
        text_plain: Some(body_text),
        text_html: Some(text_html),
        attachments,
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    };

    (envelope, body)
}

fn demo_attachments(
    message_id: &MessageId,
    current_num: usize,
    category: usize,
    flags: MessageFlags,
    has_attachments: bool,
) -> Vec<AttachmentMeta> {
    if !has_attachments {
        return Vec::new();
    }

    let mut attachments = Vec::new();
    let mut push_attachment = |filename: String,
                               mime_type: &str,
                               size_bytes: u64,
                               disposition: AttachmentDisposition,
                               content_id: Option<String>| {
        let attachment_index = attachments.len() + 1;
        let provider_id = format!("demo-att-{current_num}-{attachment_index}");
        attachments.push(AttachmentMeta {
            // Derived so regenerating a message yields the same attachment ids.
            id: AttachmentId::from_provider_id("fake", &provider_id),
            message_id: message_id.clone(),
            filename,
            mime_type: mime_type.to_string(),
            disposition,
            content_id,
            content_location: None,
            size_bytes,
            local_path: None,
            provider_id,
        });
    };

    match category {
        2 => push_attachment(
            format!("newsletter-chart-{current_num}.png"),
            "image/png",
            84_000 + (current_num as u64 % 40_000),
            AttachmentDisposition::Inline,
            Some(format!("hero-{current_num}@demo.mxr")),
        ),
        5 => {
            push_attachment(
                format!("receipt-{current_num}.pdf"),
                "application/pdf",
                140_000 + (current_num as u64 % 90_000),
                AttachmentDisposition::Attachment,
                None,
            );
            push_attachment(
                format!("line-items-{current_num}.csv"),
                "text/csv",
                18_000 + (current_num as u64 % 5_000),
                AttachmentDisposition::Attachment,
                None,
            );
        }
        6 => {
            push_attachment(
                format!("itinerary-{current_num}.pdf"),
                "application/pdf",
                220_000 + (current_num as u64 % 110_000),
                AttachmentDisposition::Attachment,
                None,
            );
            push_attachment(
                format!("demo-day-{current_num}.ics"),
                "text/calendar",
                7_200,
                AttachmentDisposition::Attachment,
                None,
            );
        }
        8 => push_attachment(
            format!("candidate-scorecard-{current_num}.pdf"),
            "application/pdf",
            96_000 + (current_num as u64 % 30_000),
            AttachmentDisposition::Attachment,
            None,
        ),
        11 => push_attachment(
            format!("promo-banner-{current_num}.png"),
            "image/png",
            92_000 + (current_num as u64 % 35_000),
            AttachmentDisposition::Inline,
            Some(format!("promo-{current_num}@demo.mxr")),
        ),
        _ if flags.contains(MessageFlags::SENT) => push_attachment(
            format!("follow-up-notes-{current_num}.md"),
            "text/markdown",
            12_000 + (current_num as u64 % 8_000),
            AttachmentDisposition::Attachment,
            None,
        ),
        _ => push_attachment(
            format!("demo-brief-{current_num}.pdf"),
            "application/pdf",
            52_000 + (current_num as u64 % 250_000),
            AttachmentDisposition::Attachment,
            None,
        ),
    }

    attachments
}

fn demo_html_body(subject: &str, body_text: &str, category: usize, current_num: usize) -> String {
    let image = if category == 2 {
        format!(r#"<img alt="Newsletter chart" src="cid:hero-{current_num}@demo.mxr" />"#)
    } else if category == 11 {
        format!(r#"<img alt="Promotional banner" src="cid:promo-{current_num}@demo.mxr" />"#)
    } else if category == 4 {
        r#"<img alt="Onboarding screenshot" src="https://demo.mxr.local/assets/onboarding-shot.png" />"#.to_string()
    } else {
        String::new()
    };
    let extra_link = match category {
        0 => {
            r#"<a href="https://demo.mxr.local/runbooks/aurora-rollout">Aurora rollout runbook</a>"#
        }
        2 => {
            r#"<a href="https://demo.mxr.local/newsletters/manage-preferences">Manage newsletter preferences</a>"#
        }
        5 => r#"<a href="https://billing.demo.mxr.local/invoices">Billing portal</a>"#,
        6 => r#"<a href="https://travel.demo.mxr.local/trips/portland">Trip details</a>"#,
        7 => r#"<a href="https://status.demo.mxr.local/incidents/sync-jobs">Incident timeline</a>"#,
        10 => r#"<a href="https://claim-now.demo.mxr.local/prize">Claim prize now</a>"#,
        11 => r#"<a href="https://promo.demo.mxr.local/offers/workspace">View promotion</a>"#,
        12 => r#"<a href="https://security-mail.demo.mxr.local/reset">Urgent password reset</a>"#,
        _ => r#"<a href="https://demo.mxr.local/docs">Project notes</a>"#,
    };
    format!(
        r#"<!doctype html><html><body><h1>{subject}</h1>{image}<p>{}</p><p>{extra_link}</p></body></html>"#,
        body_text.replace('\n', "<br />")
    )
}

fn make_label(account_id: &AccountId, name: &str, kind: LabelKind, provider_id: &str) -> Label {
    let role = match provider_id {
        "INBOX" => Some(Role::Inbox),
        "SENT" => Some(Role::Sent),
        "DRAFT" => Some(Role::Drafts),
        "TRASH" => Some(Role::Trash),
        "SPAM" => Some(Role::Spam),
        "IMPORTANT" => Some(Role::Important),
        "STARRED" => Some(Role::Starred),
        _ => None,
    };
    Label {
        id: LabelId::new(),
        account_id: account_id.clone(),
        name: name.to_string(),
        kind,
        color: label_color(provider_id).map(str::to_string),
        provider_id: provider_id.to_string(),
        unread_count: 0,
        total_count: 0,
        role,
    }
}

fn label_color(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "work" => Some("#3b82f6"),
        "product" => Some("#8b5cf6"),
        "newsletters" => Some("#22c55e"),
        "receipts" => Some("#f59e0b"),
        "travel" => Some("#06b6d4"),
        "alerts" => Some("#ef4444"),
        "hiring" => Some("#ec4899"),
        "waiting" => Some("#f97316"),
        "promotions" => Some("#14b8a6"),
        "potential_spam" => Some("#f43f5e"),
        _ => None,
    }
}

struct FixtureMessage<'a> {
    from_name: &'a str,
    from_email: &'a str,
    subject: &'a str,
    snippet: &'a str,
    body_text: &'a str,
    date: chrono::DateTime<chrono::Utc>,
    flags: MessageFlags,
    has_attachments: bool,
    unsubscribe: UnsubscribeMethod,
}

fn push_msg(
    envelopes: &mut Vec<Envelope>,
    bodies: &mut HashMap<String, MessageBody>,
    msg_num: &mut usize,
    account_id: &AccountId,
    thread_id: &ThreadId,
    message: FixtureMessage<'_>,
) {
    let FixtureMessage {
        from_name,
        from_email,
        subject,
        snippet,
        body_text,
        date,
        flags,
        has_attachments,
        unsubscribe,
    } = message;
    let msg_id = MessageId::new();
    let provider_id = format!("fake-msg-{msg_num}");
    *msg_num += 1;

    let mut attachments = vec![];
    if has_attachments {
        attachments.push(AttachmentMeta {
            id: AttachmentId::new(),
            message_id: msg_id.clone(),
            filename: format!("attachment-{msg_num}.pdf"),
            mime_type: "application/pdf".to_string(),
            disposition: AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 25000,
            local_path: None,
            provider_id: format!("att-{msg_num}"),
        });
    }

    envelopes.push(Envelope {
        id: msg_id.clone(),
        account_id: account_id.clone(),
        provider_id: provider_id.clone(),
        thread_id: thread_id.clone(),
        message_id_header: Some(format!("<msg-{msg_num}@fake.mxr>")),
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
        link_count: 0,
        body_word_count: 0,
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
        keywords: std::collections::BTreeSet::new(),
    });

    bodies.insert(
        provider_id,
        MessageBody {
            message_id: msg_id,
            text_plain: Some(body_text.to_string()),
            text_html: None,
            attachments,
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn curated_demo_seed_is_50_messages_across_15_counterparties() {
        let work_id = AccountId::from_provider_id("fake", "alex@work.demo.mxr.local");
        let personal_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let (work, work_bodies, _) = generate_demo_fixtures(&work_id, CURATED_DEMO_MESSAGE_COUNT);
        let (personal, personal_bodies, _) =
            generate_demo_fixtures(&personal_id, CURATED_DEMO_MESSAGE_COUNT);

        assert_eq!(work.len() + personal.len(), CURATED_DEMO_MESSAGE_COUNT);
        assert_eq!(
            work_bodies.len() + personal_bodies.len(),
            CURATED_DEMO_MESSAGE_COUNT
        );

        let counterparties = work
            .iter()
            .chain(personal.iter())
            .flat_map(counterparty_emails)
            .collect::<HashSet<_>>();
        // 12 curated counterparties + 3 carriers/merchants from the seeded
        // deliveries (Amazon, UPS, USPS), which land in the personal account.
        assert_eq!(counterparties.len(), 15);
        assert!(counterparties.contains("shipment-tracking@amazon.com"));
        assert!(counterparties.contains("mcinfo@ups.com"));
        assert!(counterparties.contains("auto-reply@usps.com"));
    }

    #[test]
    fn curated_demo_seed_exercises_core_demo_surfaces() {
        let account_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let (envelopes, bodies, _) =
            generate_demo_fixtures(&account_id, CURATED_DEMO_MESSAGE_COUNT);

        assert!(envelopes.iter().any(|env| env.has_attachments));
        assert!(envelopes
            .iter()
            .any(|env| env.flags.contains(MessageFlags::STARRED)));
        assert!(envelopes
            .iter()
            .any(|env| env.flags.contains(MessageFlags::SENT)));
        assert!(envelopes
            .iter()
            .any(|env| env.unsubscribe != UnsubscribeMethod::None));
        assert!(bodies.values().any(|body| body.text_html.is_some()));
    }

    fn counterparty_emails(envelope: &Envelope) -> Vec<String> {
        let self_emails = ["alex@demo.mxr.local", "alex@work.demo.mxr.local"];
        let mut emails = Vec::new();
        if !self_emails.contains(&envelope.from.email.as_str()) {
            emails.push(envelope.from.email.clone());
        }
        for address in &envelope.to {
            if !self_emails.contains(&address.email.as_str()) {
                emails.push(address.email.clone());
            }
        }
        emails
    }
}
