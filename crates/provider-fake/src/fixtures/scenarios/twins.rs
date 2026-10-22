//! Near-miss twins: pairs of emails that differ in the one fact that
//! should change the answer (or, for a robustness twin, a cue that should
//! not). The plain email comes first; the near miss says which scenario it
//! twins and what differs.
//!
//! Every pair has its own people, so one pair's history never leaks into
//! another's. Mail you sent is context and is not labelled, unless it makes
//! a promise.

use super::{
    locale_of, sender_kind, AskKind, Due, Expect, Home, Item, Labelled, Review, SenderKind, Who,
};
use crate::fixtures::{build_demo_msg, demo_message_id_header, DemoMessage};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{
    Address, CalendarMetadata, CalendarPerson, Envelope, MessageBody, MessageFlags,
    UnsubscribeMethod,
};

pub(super) struct Twins {
    pub messages: Vec<(Envelope, MessageBody)>,
    pub labels: Vec<Labelled>,
}

pub(super) const fn expect(who: Who, home: Home, record: bool) -> Expect {
    Expect::new(who, home, record)
}

pub(super) const CONVERSATION: Expect = expect(Who::Person, Home::Messages, false);
/// A person's message that asks a question or for an answer: your turn.
pub(super) const QUESTION: Expect = CONVERSATION.replying();
pub(super) const COPIED: Expect = expect(Who::Person, Home::Updates, false);
pub(super) const NOTICE: Expect = expect(Who::Machine, Home::Updates, false);
pub(super) const FILED: Expect = expect(Who::Machine, Home::Updates, true);
/// A machine's notice that asks you to act; its items say what it asks.
pub(super) const ACTION: Expect = expect(Who::Machine, Home::Updates, false);
pub(super) const READING: Expect = expect(Who::List, Home::Reading, false);

pub(super) fn addr(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

/// One email to build.
pub(super) struct Mail {
    thread: &'static str,
    from: Address,
    to: Vec<Address>,
    cc: Vec<Address>,
    subject: String,
    body: String,
    hours_ago: i64,
    unsubscribe: bool,
    headers: Vec<(&'static str, String)>,
    calendar: Option<CalendarMetadata>,
    reply_to: Option<Placed>,
    spam: bool,
}

impl Mail {
    /// An email to the account owner, unless `to` says otherwise.
    pub(super) fn new(
        thread: &'static str,
        from: Address,
        subject: &str,
        body: &str,
        hours_ago: i64,
    ) -> Self {
        Self {
            thread,
            from,
            to: Vec::new(),
            cc: Vec::new(),
            subject: subject.to_string(),
            body: body.to_string(),
            hours_ago,
            unsubscribe: false,
            headers: Vec::new(),
            calendar: None,
            reply_to: None,
            spam: false,
        }
    }

    pub(super) fn to(mut self, to: Vec<Address>) -> Self {
        self.to = to;
        self
    }

    pub(super) fn cc(mut self, cc: Vec<Address>) -> Self {
        self.cc = cc;
        self
    }

    pub(super) fn unsubscribe(mut self) -> Self {
        self.unsubscribe = true;
        self
    }

    pub(super) fn header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.to_string()));
        self
    }

    pub(super) fn calendar(mut self, calendar: CalendarMetadata) -> Self {
        self.calendar = Some(calendar);
        self
    }

    pub(super) fn reply_to(mut self, earlier: &Placed) -> Self {
        self.reply_to = Some(earlier.clone());
        self
    }

    pub(super) fn spam(mut self) -> Self {
        self.spam = true;
        self
    }
}

/// Where a built email ended up.
#[derive(Clone)]
pub(super) struct Placed {
    provider_id: String,
    from_email: String,
    header: String,
}

/// What a scenario says about the email it labels.
pub(super) struct Scenario {
    id: &'static str,
    description: &'static str,
    expect: Expect,
    items: Vec<Item>,
    outbound: bool,
    locale: Option<&'static str>,
    held_out: bool,
    twin_of: Option<&'static str>,
    differs_by: &'static str,
    review: Option<Review>,
}

impl Scenario {
    pub(super) fn new(id: &'static str, description: &'static str, expect: Expect) -> Self {
        Self {
            id,
            description,
            expect,
            items: Vec::new(),
            outbound: false,
            locale: None,
            held_out: false,
            twin_of: None,
            differs_by: "",
            review: None,
        }
    }

    /// The email asks this of you.
    pub(super) fn asks(mut self, items: Vec<Item>) -> Self {
        self.items = items;
        self
    }

    /// The email is mail you sent, labelled for the promise it makes.
    pub(super) fn promises(mut self, items: Vec<Item>) -> Self {
        self.items = items;
        self.outbound = true;
        self
    }

    /// The wording is in this locale, which the sender's domain does not say.
    pub(super) fn locale(mut self, locale: &'static str) -> Self {
        self.locale = Some(locale);
        self
    }

    /// The email belongs to a template family held out of the corpus the rest
    /// of the eval's authors see; the eval reports it apart.
    pub(super) fn held_out(mut self) -> Self {
        self.held_out = true;
        self
    }

    pub(super) fn twin_of(mut self, of: &'static str, differs_by: &'static str) -> Self {
        self.twin_of = Some(of);
        self.differs_by = differs_by;
        self
    }

    /// The answers in `questions` are a judgement call, for `why`.
    pub(super) fn review(mut self, questions: &'static [&'static str], why: &'static str) -> Self {
        self.review = Some(Review { questions, why });
        self
    }
}

pub(super) struct Builder {
    account_id: AccountId,
    pub(super) me: Address,
    pub(super) now: DateTime<Utc>,
    next: usize,
    messages: Vec<(Envelope, MessageBody)>,
    labels: Vec<Labelled>,
}

impl Builder {
    /// The day `days` from the seed's `now`, as "3 November". Every date a
    /// detector may read is derived from `now`, never written absolutely,
    /// so a scenario cannot expire as the real date moves on.
    pub(super) fn on(&self, days: i64) -> String {
        (self.now + Duration::days(days))
            .format("%-d %B")
            .to_string()
    }

    /// Build `mail` as inbound mail.
    pub(super) fn inbound(&mut self, mail: Mail) -> Placed {
        self.put(mail, false)
    }

    /// Build mail the account owner sent: context, labelled only when it
    /// makes a promise.
    pub(super) fn sent(
        &mut self,
        thread: &'static str,
        to: Address,
        subject: &str,
        body: &str,
        hours_ago: i64,
    ) -> Placed {
        let mail = Mail::new(thread, self.me.clone(), subject, body, hours_ago).to(vec![to]);
        self.put(mail, true)
    }

    fn put(&mut self, mail: Mail, outbound: bool) -> Placed {
        let num = self.next;
        self.next += 1;
        let thread_id = ThreadId::from_scoped_provider_id(
            &self.account_id,
            "fake",
            &format!("demo-twin-{}", mail.thread),
        );
        let to = if mail.to.is_empty() {
            vec![self.me.clone()]
        } else {
            mail.to
        };
        let flags = if outbound {
            MessageFlags::READ | MessageFlags::SENT
        } else if mail.spam {
            MessageFlags::SPAM
        } else {
            MessageFlags::empty()
        };
        let label = if outbound {
            "SENT"
        } else if mail.spam {
            "SPAM"
        } else {
            "INBOX"
        };
        let (in_reply_to, references) = match &mail.reply_to {
            Some(earlier) => (Some(earlier.header.clone()), vec![earlier.header.clone()]),
            None => (None, Vec::new()),
        };
        let (envelope, mut body) = build_demo_msg(
            num,
            &self.account_id,
            &thread_id,
            DemoMessage {
                from: mail.from.clone(),
                to,
                cc: mail.cc,
                snippet: mail.body.chars().take(120).collect(),
                subject: mail.subject,
                body_text: mail.body,
                date: self.now - Duration::hours(mail.hours_ago),
                flags,
                has_attachments: false,
                category: 9,
                unsubscribe: if mail.unsubscribe {
                    UnsubscribeMethod::OneClick {
                        url: format!(
                            "https://{}/unsubscribe",
                            mail.from.email.split('@').nth(1).unwrap_or("example.com")
                        ),
                    }
                } else {
                    UnsubscribeMethod::None
                },
                label_provider_ids: vec![label.to_string()],
                in_reply_to,
                references,
            },
        );
        if !mail.headers.is_empty() {
            body.metadata.raw_headers = Some(
                mail.headers
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\n"))
                    .collect(),
            );
            body.metadata.list_id = mail
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("List-Id"))
                .map(|(_, value)| value.clone());
        }
        body.metadata.calendar = mail.calendar;
        let placed = Placed {
            provider_id: envelope.provider_id.clone(),
            from_email: envelope.from.email.clone(),
            header: demo_message_id_header(num),
        };
        self.messages.push((envelope, body));
        placed
    }

    pub(super) fn label(&mut self, placed: &Placed, scenario: Scenario) {
        self.labels.push(Labelled {
            provider_id: placed.provider_id.clone(),
            from_email: placed.from_email.clone(),
            scenario: scenario.id,
            description: scenario.description,
            expect: scenario.expect,
            items: scenario.items,
            outbound: scenario.outbound,
            sender: if scenario.outbound {
                // The owner writes as a person.
                SenderKind::Person
            } else {
                sender_kind(scenario.expect.who, &placed.from_email)
            },
            locale: scenario
                .locale
                .unwrap_or_else(|| locale_of(&placed.from_email)),
            family: scenario.id,
            twin_of: scenario.twin_of,
            differs_by: scenario.differs_by,
            review: scenario.review,
            held_out: scenario.held_out,
        });
    }
}

pub(super) fn build(
    account_id: &AccountId,
    me: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Twins {
    let mut b = Builder {
        account_id: account_id.clone(),
        me: me.clone(),
        now,
        next: first_num,
        messages: Vec::new(),
        labels: Vec::new(),
    };
    newsletter_from_a_correspondent(&mut b);
    receipt_that_asks_a_question(&mut b);
    no_need_to_reply(&mut b);
    copied_cc_or_named_in_to(&mut b);
    no_reply_ack_or_human_reply(&mut b);
    role_address_you_never_wrote_to(&mut b);
    bank_alert_or_friend_reply(&mut b);
    billing_document_or_promotion(&mut b);
    invite_or_invite_reply(&mut b);
    delivery_or_carrier_marketing(&mut b);
    failed_payment_or_receipt(&mut b);
    dated_bill_or_statement_ready(&mut b);
    verification_code_or_friend_with_a_code(&mut b);
    out_of_office_or_real_reply(&mut b);
    receipt_with_unsubscribe_header(&mut b);
    mailing_list_post_or_direct(&mut b);
    application_ack_or_recruiter_reply(&mut b);
    comment_notification_or_colleague(&mut b);
    renewal_notice_or_card_expiring(&mut b);
    sign_request_or_completed(&mut b);
    forwarded_newsletter_or_publisher(&mut b);
    booking_or_booking_reminder_marketing(&mut b);
    direct_or_crowd(&mut b);
    inbox_or_spam_folder(&mut b);
    super::twins_asks::build(&mut b);
    super::held_out::families(&mut b);
    Twins {
        messages: b.messages,
        labels: b.labels,
    }
}

/// A person you write to sends a newsletter.
fn newsletter_from_a_correspondent(b: &mut Builder) {
    let dana = addr("Dana Reyes", "dana@brightwell.example");
    let asked = b.sent(
        "p01-catchup",
        dana.clone(),
        "Catch-up next week?",
        "Hi Dana,\n\nAre you around for a catch-up next week?\n\nAlex",
        130,
    );
    let plain = b.inbound(
        Mail::new(
            "p01-catchup",
            dana.clone(),
            "Re: Catch-up next week?",
            "Hi Alex,\n\nThursday works for me. Shall we say 3pm?\n\nDana",
            20,
        )
        .reply_to(&asked),
    );
    b.label(
        &plain,
        Scenario::new(
            "correspondent-note",
            "Someone you write to answers your question",
            QUESTION,
        ),
    );
    let newsletter = b.inbound(
        Mail::new(
            "p01-newsletter",
            dana,
            &format!("Brightwell notes: {}", b.now.format("%B")),
            "Hi friends,\n\nThis month at Brightwell: two new workshops and a reading list.\n\nDana",
            10,
        )
        .unsubscribe(),
    );
    b.label(
        &newsletter,
        Scenario::new(
            "correspondent-newsletter",
            "The same person you write to sends a newsletter with List-Unsubscribe",
            READING,
        )
        .twin_of("correspondent-note", "carries List-Unsubscribe"),
    );
}

/// A receipt, with and without an ask.
fn receipt_that_asks_a_question(b: &mut Builder) {
    let shop = addr("Fernhill Books", "orders@fernhillbooks.example");
    let plain = b.inbound(Mail::new(
        "p02-receipt",
        shop.clone(),
        "Your Fernhill Books receipt",
        "Thanks for your order.\nOrder number: FB-20931\nTotal: £14.00\nYour books are on the way.",
        8,
    ));
    b.label(
        &plain,
        Scenario::new("receipt-plain", "A shop's plain order receipt", FILED),
    );
    let asks = b.inbound(Mail::new(
        "p02-receipt-asks",
        shop,
        "Your Fernhill Books receipt",
        "Thanks for your order.\nOrder number: FB-20932\nTotal: £14.00\nYour books are on the way.\n\nCan you confirm your delivery address by Friday? Reply to this email with the address.",
        7,
    ));
    b.label(
        &asks,
        Scenario::new(
            "receipt-asks-for-an-address",
            "The same receipt, which also asks you to confirm your delivery address by Friday",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Provide, "Fernhill Books").due(Due::Words("Friday"))])
        .twin_of("receipt-plain", "asks for an action with a due word")
        .review(
            &["who", "home", "todo"],
            "The ask has a verb, an object and a due word, so labelled a to-do; whether a question inside a receipt is a to-do or only an unanswered question is a policy call. The receipt's £14.00 total is not an amount owed, so the item has no amount."),
    );
}

/// A colleague's FYI against a colleague's ask.
fn no_need_to_reply(b: &mut Builder) {
    let tobias = addr("Tobias Frye", "tobias@quillworks.example");
    let ask = b.inbound(Mail::new(
        "p03-ask",
        tobias.clone(),
        "Q3 figures",
        "Hi Alex,\n\nCould you send me the Q3 figures when you have a minute?\n\nTobias",
        6,
    ));
    b.label(
        &ask,
        Scenario::new(
            "colleague-asks",
            "A colleague asks you for something",
            CONVERSATION,
        )
        .asks(vec![Item::asks(AskKind::Send, "Tobias Frye")])
        .review(
            &["todo"],
            "\"Could you send me the Q3 figures\" is a pending request of you with an action other than a reply, so blueprint 37 makes it an item even with no date. The earlier label said no to-do.",
        ),
    );
    let fyi = b.inbound(
        Mail::new(
            "p03-fyi",
            tobias,
            "Q3 figures are in the shared folder",
            "FYI: the Q3 figures are in the shared folder.\nNo need to reply.\n\nTobias",
            5,
        )
        .to(vec![
            b.me.clone(),
            addr("Ines Calder", "ines@quillworks.example"),
            addr("Kofi Mensah", "kofi@quillworks.example"),
        ]),
    );
    b.label(
        &fyi,
        Scenario::new(
            "colleague-fyi-no-need-to-reply",
            "The same colleague sends an FYI to three people and says there is no need to reply",
            COPIED,
        )
        .twin_of("colleague-asks", "an FYI to a group that says no need to reply")
        .review(
            &["who", "home"],
            "A person's FYI with no ask to a group is information, so its home is Updates (the copied rule), though the sender stays a person. Today's code treats FYI framing as machine-like (amended D119); whether Who should say person or machine here is the open call."),
    );
}

/// Copied against named in To.
fn copied_cc_or_named_in_to(b: &mut Builder) {
    let priya = addr("Priya Nair", "priya@harrowgate.example");
    let ruth = addr("Ruth Vega", "ruth@keystone.example");
    b.sent(
        "p04-intro",
        priya.clone(),
        "Offsite planning",
        "Hi Priya,\n\nHappy to help with the offsite. Tell me what you need.\n\nAlex",
        140,
    );
    let copied = b.inbound(
        Mail::new(
            "p04-copied",
            priya.clone(),
            "Venue for the offsite",
            "Ruth, I've pencilled the venue for the 20th. Alex, copying you so you have the dates.\n\nPriya",
            9,
        )
        .to(vec![ruth.clone()])
        .cc(vec![b.me.clone()]),
    );
    b.label(
        &copied,
        Scenario::new(
            "thread-copied-only",
            "Someone you write to writes to a colleague and only copies you",
            COPIED,
        ),
    );
    let named = b.inbound(
        Mail::new(
            "p04-named",
            priya,
            "Venue for the offsite, second option",
            "Alex, Ruth, I've pencilled a second venue for the 20th. Does that work for you both?\n\nPriya",
            8,
        )
        .to(vec![b.me.clone(), ruth]),
    );
    b.label(
        &named,
        Scenario::new(
            "thread-names-you-in-to",
            "The same person writes to you and a colleague, naming you in To",
            QUESTION,
        )
        .twin_of("thread-copied-only", "you are in To, not only Cc"),
    );
}

/// A no-reply acknowledgement against a person's reply, same thread.
fn no_reply_ack_or_human_reply(b: &mut Builder) {
    let asked = b.sent(
        "p05-ticket",
        addr("Pelican support", "help@pelican-hosting.example"),
        "Can't sign in",
        "Hi, I can't sign in to my account since yesterday. Can you help?\n\nAlex",
        100,
    );
    let ack = b.inbound(
        Mail::new(
            "p05-ticket",
            addr("Pelican Hosting", "no-reply@pelican-hosting.example"),
            "We received your request",
            "Hi Alex,\n\nWe received your request and will reply within 2 business days.\nTicket: PH-4412",
            99,
        )
        .header("Auto-Submitted", "auto-replied")
        .reply_to(&asked),
    );
    b.label(
        &ack,
        Scenario::new(
            "support-auto-ack",
            "A no-reply acknowledgement of your support request",
            NOTICE,
        ),
    );
    let human = b.inbound(
        Mail::new(
            "p05-ticket",
            addr("Maria Lopes", "maria@pelican-hosting.example"),
            "Re: Can't sign in",
            "Hi Alex,\n\nI've reset your password. Can you try signing in again and tell me if it works?\n\nMaria",
            20,
        )
        .reply_to(&asked),
    );
    b.label(
        &human,
        Scenario::new(
            "support-human-reply",
            "A support agent answers the same request personally",
            QUESTION,
        )
        .review(
            &["todo"],
            "\"Can you try signing in again and tell me if it works?\" asks for an action and then an answer; the answer is what the agent waits for, so it is labelled Your turn and no item.",
        )
        .twin_of(
            "support-auto-ack",
            "written by a person, not a no-reply system",
        ),
    );
}

/// A role address you never wrote to against one you correspond with.
fn role_address_you_never_wrote_to(b: &mut Builder) {
    let cold = b.inbound(Mail::new(
        "p06-cold",
        addr("Harbourlight", "support@harbourlight.example"),
        "Welcome to Harbourlight",
        "Hi Alex,\n\nThanks for signing up. Reply to this email if you need a hand getting started.\n\nThe Harbourlight support team",
        12,
    ));
    b.label(
        &cold,
        Scenario::new(
            "role-address-never-written-to",
            "A support@ address you have never written to sends a welcome",
            NOTICE,
        ),
    );
    let asked = b.sent(
        "p06-billing",
        addr("Kestrel support", "support@kestrelcloud.example"),
        "Billing question",
        &format!(
            "Hi, I was charged twice in {}. Could you check?\n\nAlex",
            b.now.format("%B")
        ),
        100,
    );
    let reply = b.inbound(
        Mail::new(
            "p06-billing",
            addr("Kestrel support", "support@kestrelcloud.example"),
            "Re: Billing question",
            "Hi Alex,\n\nWe've refunded the second charge. It will show in 3 to 5 days.\n\nKestrel support",
            30,
        )
        .reply_to(&asked),
    );
    b.label(
        &reply,
        Scenario::new(
            "role-address-you-correspond-with",
            "A support@ address answers a question you wrote to it",
            CONVERSATION,
        )
        .twin_of(
            "role-address-never-written-to",
            "you wrote to this address and it is answering",
        ),
    );
}

/// A bank alert against a friend's reply with the same shape of subject.
fn bank_alert_or_friend_reply(b: &mut Builder) {
    let alert = b.inbound(Mail::new(
        "p07-alert",
        addr("Northbank", "service@northbank.example"),
        "Card ending 4417: £40.00 spent at Tesco",
        "A payment of £40.00 was made with your card ending 4417 at Tesco.",
        4,
    ));
    b.label(
        &alert,
        Scenario::new(
            "bank-alert",
            "A card alert whose subject has an amount and a masked card number",
            NOTICE,
        ),
    );
    let jess = addr("Jess Okoro", "jess@okoro.example");
    let asked = b.sent(
        "p07-tickets",
        jess.clone(),
        "Concert tickets",
        "Jess, did the tickets come through?\n\nAlex",
        50,
    );
    let reply = b.inbound(
        Mail::new(
            "p07-tickets",
            jess,
            "Re: Concert tickets, paid £40 on card ending 4417?",
            "Hi Alex,\n\nYes, I paid £40 on the card ending 4417. Can you send me your half?\n\nJess",
            3,
        )
        .reply_to(&asked),
    );
    b.label(
        &reply,
        Scenario::new(
            "friend-reply-about-a-payment",
            "A friend's reply whose subject also has an amount and a masked card number",
            CONVERSATION,
        )
        .asks(vec![Item::asks(AskKind::Pay, "Jess Okoro")])
        .review(
            &["todo"],
            "\"Can you send me your half?\" asks the owner to pay a friend. No amount is stated (the £40 is the whole), so the item has none.",
        )
        .twin_of("bank-alert", "a person's reply, not a bank's notification"),
    );
}

/// A billing document against marketing that mentions an amount.
fn billing_document_or_promotion(b: &mut Builder) {
    let issued = (b.now - Duration::days(1)).format("%Y-%m-%d");
    let invoice = b.inbound(Mail::new(
        "p08-invoice",
        addr("Fernhill Hosting", "accounts@fernhillhosting.example"),
        "Your invoice INV-2041 for £48.00",
        &format!("Thank you for your order.\nInvoice number: INV-2041\nDate of issue: {issued}\nAmount due: £48.00\nYour invoice is attached; we'll collect it by direct debit."),
        15,
    ));
    b.label(
        &invoice,
        Scenario::new(
            "billing-document",
            "An invoice with a number, date and amount, collected by direct debit",
            FILED,
        ),
    );
    let promo = b.inbound(
        Mail::new(
            "p08-promo",
            addr("Fernhill Hosting", "offers@fernhillhosting.example"),
            "Save £48 on your next order",
            "Hi Alex,\n\nTake £48 off when you spend £200 this week only.\n\nShop now: https://fernhillhosting.example/offer",
            14,
        )
        .unsubscribe(),
    );
    b.label(
        &promo,
        Scenario::new(
            "marketing-with-an-amount",
            "The same company's promotion with an amount in the subject and List-Unsubscribe",
            READING,
        )
        .twin_of("billing-document", "a promotion, not a document of record"),
    );
}

/// An invite against the reply to one.
fn invite_or_invite_reply(b: &mut Builder) {
    let start = b.now + Duration::days(7);
    let end = start + Duration::hours(1);
    let (starts_at, ends_at) = (
        start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    );
    let when = start.format("%a %H:%M");
    let invite = b.inbound(
        Mail::new(
            "p09-invite",
            addr("Lena Hart", "lena@moorfield.example"),
            &format!("Invitation: Design review @ {when}"),
            &format!(
                "Lena Hart invited you to Design review.\n{} to {}.",
                start.format("%A %H:%M"),
                end.format("%H:%M")
            ),
            11,
        )
        .calendar(CalendarMetadata {
            method: Some("REQUEST".to_string()),
            summary: Some("Design review".to_string()),
            component_kind: Some("VEVENT".to_string()),
            uid: Some("design-review-p09@moorfield.example".to_string()),
            sequence: Some(0),
            starts_at: Some(starts_at.clone()),
            ends_at: Some(ends_at.clone()),
            organizer: Some(CalendarPerson {
                email: "lena@moorfield.example".to_string(),
                name: Some("Lena Hart".to_string()),
                uri: None,
            }),
            rsvp_requested: true,
            ..Default::default()
        }),
    );
    b.label(
        &invite,
        Scenario::new(
            "calendar-invite",
            "A calendar invitation that asks for an RSVP",
            ACTION,
        )
        .asks(vec![Item::asks(AskKind::Attend, "Lena Hart").due(Due::Stated)])
        .review(
            &["todo"],
            "Invites have a home in Updates (blueprint 32). Whether an invitation that asks for an RSVP is a to-do is a policy call; labelled yes because an RSVP is a to-do kind."),
    );
    let reply = b.inbound(
        Mail::new(
            "p09-invite-reply",
            addr("Kofi Adeyemi", "kofi@moorfield.example"),
            &format!("Accepted: Design review @ {when}"),
            "Kofi Adeyemi has accepted this invitation.",
            9,
        )
        .calendar(CalendarMetadata {
            method: Some("REPLY".to_string()),
            summary: Some("Design review".to_string()),
            component_kind: Some("VEVENT".to_string()),
            uid: Some("design-review-p09@moorfield.example".to_string()),
            sequence: Some(0),
            starts_at: Some(starts_at.clone()),
            ends_at: Some(ends_at.clone()),
            organizer: Some(CalendarPerson {
                email: "lena@moorfield.example".to_string(),
                name: Some("Lena Hart".to_string()),
                uri: None,
            }),
            rsvp_requested: false,
            ..Default::default()
        }),
    );
    b.label(
        &reply,
        Scenario::new(
            "calendar-invite-reply",
            "An attendee's answer to the same invitation",
            NOTICE,
        )
        .twin_of(
            "calendar-invite",
            "an answer to an invitation, nothing to RSVP to",
        ),
    );
}

/// A delivery against the carrier's marketing.
fn delivery_or_carrier_marketing(b: &mut Builder) {
    let delivery = b.inbound(Mail::new(
        "p10-delivery",
        addr("UPS", "pkginfo@ups.com"),
        "UPS Update: your Moorfield Books package is on the way",
        "Hello Alex,\n\nYour order from Moorfield Books has shipped and is on the way.\n\nMerchant: Moorfield Books\nTracking number: 1Z999AA10123456784\nScheduled delivery: Friday by end of day",
        7,
    ));
    b.label(
        &delivery,
        Scenario::new(
            "carrier-delivery",
            "A carrier's tracking update for a package on its way",
            NOTICE,
        ),
    );
    let promo = b.inbound(
        Mail::new(
            "p10-promo",
            addr("UPS My Choice", "marketing@ups.com"),
            "Ship smarter: 20% off your next label",
            "Hi Alex,\n\nGet 20% off your next shipping label with UPS My Choice Plus.",
            6,
        )
        .unsubscribe(),
    );
    b.label(
        &promo,
        Scenario::new(
            "carrier-marketing",
            "The same carrier's promotion, with List-Unsubscribe",
            READING,
        )
        .twin_of("carrier-delivery", "a promotion, not about your package"),
    );
}

/// A failed payment against a receipt.
fn failed_payment_or_receipt(b: &mut Builder) {
    let gym = addr("Hollis Gym", "no-reply@hollisgym.example");
    let failed = b.inbound(Mail::new(
        "p11-failed",
        gym.clone(),
        "Your payment didn't go through",
        "Hi Alex,\n\nWe couldn't take your £29.00 membership payment. Update your card to keep your membership: https://hollisgym.example/billing",
        5,
    ));
    b.label(
        &failed,
        Scenario::new(
            "payment-failed",
            "A gym could not take the membership payment",
            ACTION,
        )
        .asks(vec![
            Item::asks(AskKind::FixPayment, "Hollis Gym").amount(2_900, "GBP")
        ]),
    );
    let paid = b.inbound(Mail::new(
        "p11-paid",
        gym,
        "Payment received, thank you",
        "Hi Alex,\n\nWe've taken your £29.00 membership payment.\nTotal: £29.00",
        4,
    ));
    b.label(
        &paid,
        Scenario::new(
            "payment-received",
            "The same gym confirms the payment",
            FILED,
        )
        .twin_of("payment-failed", "the payment succeeded"),
    );
}

/// A bill with a due date against a notice that a statement is ready.
fn dated_bill_or_statement_ready(b: &mut Builder) {
    let due = (b.now + Duration::days(9)).format("%-d %B %Y").to_string();
    let bill = b.inbound(Mail::new(
        "p12-bill",
        addr("Brightgrid Energy", "billing@brightgrid.example"),
        "Your electricity bill is ready",
        &format!("Hi Alex,\n\nYour bill of £92.40 is due on {due}.\nPay online: https://brightgrid.example/pay\nAccount number: BG-77310"),
        13,
    ));
    b.label(
        &bill,
        Scenario::new(
            "dated-bill",
            "An energy bill with an amount and a due date nine days away",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Brightgrid Energy")
            .amount(9_240, "GBP")
            .due(Due::Stated)
            .reference("BG-77310")]),
    );
    let statement = b.inbound(Mail::new(
        "p12-statement",
        addr("Brightgrid Energy", "statements@brightgrid.example"),
        "Your statement is ready",
        &format!(
            "Hi Alex,\n\nYour {} statement is ready to view in your account. Nothing to pay.",
            b.now.format("%B")
        ),
        12,
    ));
    b.label(
        &statement,
        Scenario::new(
            "statement-ready-notice",
            "The same company says a statement is ready to view; nothing to pay",
            NOTICE,
        )
        .twin_of("dated-bill", "no amount due and no due date")
        .review(
            &["record"],
            "Same call as todo-statement-ready: a notice that a statement exists is labelled no record."),
    );
}

/// A system's code against a friend who sends a code.
fn verification_code_or_friend_with_a_code(b: &mut Builder) {
    let code = b.inbound(Mail::new(
        "p13-code",
        addr("Pinecone Login", "verify@pinecone-login.example"),
        "Your verification code is 731204",
        "Your code is 731204. It expires in 10 minutes.",
        2,
    ));
    b.label(
        &code,
        Scenario::new(
            "verification-code",
            "A login system's verification code",
            NOTICE,
        ),
    );
    let mo = addr("Mo Haddad", "mo@haddad.example");
    let asked = b.sent(
        "p13-wifi",
        mo.clone(),
        "Wifi at the cottage",
        "Mo, what's the wifi code at the cottage?\n\nAlex",
        30,
    );
    let friend = b.inbound(
        Mail::new(
            "p13-wifi",
            mo,
            "Re: Wifi at the cottage",
            "Hi Alex,\n\nThe code is 731204. Don't tell anyone!\n\nMo",
            1,
        )
        .reply_to(&asked),
    );
    b.label(
        &friend,
        Scenario::new(
            "friend-sends-a-code",
            "A friend answers your question with a code in the text",
            CONVERSATION,
        )
        .twin_of("verification-code", "a person answering you"),
    );
}

/// An out-of-office against the real answer.
fn out_of_office_or_real_reply(b: &mut Builder) {
    let back = b.on(10);
    let renewal = b.on(30);
    let nia = addr("Nia Okoye", "nia@lanternworks.example");
    let asked = b.sent(
        "p14-renewal",
        nia.clone(),
        "Contract question",
        "Nia, could you confirm the renewal date?\n\nAlex",
        60,
    );
    let away = b.inbound(
        Mail::new(
            "p14-renewal",
            nia.clone(),
            "Automatic reply: Contract question",
            &format!("Hi, I'm away until {back} with limited access to email. For anything urgent contact my colleague.\n\nNia"),
            59,
        )
        .header("Auto-Submitted", "auto-replied")
        .reply_to(&asked),
    );
    b.label(
        &away,
        Scenario::new(
            "out-of-office",
            "Someone you wrote to sends an automatic out-of-office reply",
            NOTICE,
        ),
    );
    let real = b.inbound(
        Mail::new(
            "p14-renewal",
            nia,
            "Re: Contract question",
            &format!("Hi Alex,\n\nThe renewal date is {renewal}.\n\nNia"),
            20,
        )
        .reply_to(&asked),
    );
    b.label(
        &real,
        Scenario::new(
            "real-reply-after-out-of-office",
            "The same person answers for real",
            CONVERSATION,
        )
        .twin_of(
            "out-of-office",
            "no Auto-Submitted header; a person wrote it",
        ),
    );
}

/// A receipt that carries List-Unsubscribe against the shop's promotion.
fn receipt_with_unsubscribe_header(b: &mut Builder) {
    let receipt = b.inbound(
        Mail::new(
            "p15-receipt",
            addr("Kiln & Clay", "orders@kiln-and-clay.example"),
            "Your Kiln & Clay receipt",
            "Thanks for your order.\nOrder number: KC-5521\nOrder total: £36.00",
            16,
        )
        .unsubscribe(),
    );
    b.label(
        &receipt,
        Scenario::new(
            "receipt-with-unsubscribe-header",
            "A shop's order receipt whose sending service adds List-Unsubscribe",
            FILED,
        ),
    );
    let promo = b.inbound(
        Mail::new(
            "p15-promo",
            addr("Kiln & Clay", "hello@kiln-and-clay.example"),
            "New glazes just landed",
            "Hi Alex,\n\nOur autumn glazes are in. Browse the collection on our site.",
            15,
        )
        .unsubscribe(),
    );
    b.label(
        &promo,
        Scenario::new(
            "shop-promotion",
            "The same shop's promotion, with the same header",
            READING,
        )
        .twin_of(
            "receipt-with-unsubscribe-header",
            "marketing, not a receipt",
        ),
    );
}

/// The same person writing directly and through a list.
fn mailing_list_post_or_direct(b: &mut Builder) {
    let femi = addr("Femi Adeyemi", "femi@oakline.example");
    let direct = b.inbound(Mail::new(
        "p16-direct",
        femi.clone(),
        "Lunch on Friday?",
        "Hi Alex,\n\nAre you free for lunch on Friday?\n\nFemi",
        18,
    ));
    b.label(
        &direct,
        Scenario::new(
            "person-writes-directly",
            "A person invites you to lunch",
            QUESTION,
        ),
    );
    let post = b.inbound(
        Mail::new(
            "p16-list",
            femi,
            "[oakline-dev] Lunch on Friday?",
            "Hi all,\n\nAre people free for lunch on Friday?\n\nFemi",
            17,
        )
        .header("List-Id", "<dev.oakline.example>"),
    );
    b.label(
        &post,
        Scenario::new(
            "person-posts-to-a-list",
            "The same person posts the same question to a mailing list",
            READING,
        )
        .twin_of(
            "person-writes-directly",
            "arrives through a mailing list (List-Id)",
        ),
    );
}

/// An application acknowledgement against a recruiter's reply, both with an opt-out header.
fn application_ack_or_recruiter_reply(b: &mut Builder) {
    let sasha = addr("Sasha Wren", "sasha@talentbridge.example");
    let applied = b.sent(
        "p17-application",
        sasha.clone(),
        "Application: Staff Engineer",
        "Hi Sasha,\n\nI'd like to apply for the Staff Engineer role. CV attached.\n\nAlex",
        90,
    );
    let reply = b.inbound(
        Mail::new(
            "p17-application",
            sasha,
            "Re: Application: Staff Engineer",
            "Hi Alex,\n\nThanks for applying. Could you share your availability for a call next week?\n\nSasha",
            19,
        )
        .unsubscribe()
        .reply_to(&applied),
    );
    b.label(
        &reply,
        Scenario::new(
            "recruiter-reply-with-opt-out",
            "A recruiter answers your application personally; the tool adds List-Unsubscribe",
            QUESTION,
        ),
    );
    let ack = b.inbound(
        Mail::new(
            "p17-ack",
            addr("Talentbridge Jobs", "jobs@talentbridge.example"),
            "We received your application",
            "Hi Alex,\n\nThanks for applying for Staff Engineer. We'll be in touch.",
            88,
        )
        .unsubscribe(),
    );
    b.label(
        &ack,
        Scenario::new(
            "application-auto-ack",
            "An automatic acknowledgement of your application, with the same header",
            NOTICE,
        )
        .twin_of("recruiter-reply-with-opt-out", "a system's acknowledgement, not a person's reply")
        .review(
            &["who", "home"],
            "A transactional acknowledgement is a notification (Updates), not a newsletter, even with List-Unsubscribe. Today's code reads the header as a list."),
    );
}

/// A comment notification against the colleague's own mail.
fn comment_notification_or_colleague(b: &mut Builder) {
    let notification = b.inbound(Mail::new(
        "p18-comment",
        addr("Google Docs", "comments-noreply@docs.google.com"),
        "Ellis Moran commented on Q4 plan",
        "Ellis Moran left a comment on the Q4 plan: \"Can we move the launch?\"\n\nOpen the document.",
        3,
    ));
    b.label(
        &notification,
        Scenario::new(
            "document-comment-notification",
            "A document service says a colleague commented",
            NOTICE,
        ),
    );
    let colleague = b.inbound(Mail::new(
        "p18-colleague",
        addr("Ellis Moran", "ellis@pinebank.example"),
        "Q4 plan",
        "Hi Alex,\n\nI left comments on the Q4 plan. Could you take a look?\n\nEllis",
        2,
    ));
    b.label(
        &colleague,
        Scenario::new(
            "colleague-asks-about-a-document",
            "The colleague writes to you about the same document",
            CONVERSATION,
        )
        .asks(vec![Item::asks(AskKind::Other, "Ellis Moran")])
        .review(
            &["todo"],
            "\"Could you take a look?\" asks for a review of a document, an action other than a reply, so it is an item of kind other.",
        )
        .twin_of(
            "document-comment-notification",
            "a person writing, not a service",
        ),
    );
}

/// A renewal notice against a card about to expire.
fn renewal_notice_or_card_expiring(b: &mut Builder) {
    let renews_on = b.on(14);
    let card_expires = b.on(10);
    let renews = b.inbound(Mail::new(
        "p19-renews",
        addr("Cloudbox", "billing@cloudbox.example"),
        &format!("Your Cloudbox subscription renews on {renews_on}"),
        &format!("Hi Alex,\n\nYour subscription renews on {renews_on} for £9.99. We'll charge your card on file automatically; there's nothing to do."),
        21,
    ));
    b.label(
        &renews,
        Scenario::new(
            "renewal-notice",
            "A subscription renewal notice that says nothing needs doing",
            NOTICE,
        ),
    );
    let expiring = b.inbound(Mail::new(
        "p19-card",
        addr("Cloudbox", "billing@cloudbox.example"),
        "Your card on file expires soon",
        &format!("Hi Alex,\n\nThe card we have on file expires on {card_expires}. Update your payment method before {renews_on} or your subscription will lapse: https://cloudbox.example/billing"),
        20,
    ));
    b.label(
        &expiring,
        Scenario::new(
            "card-expiring",
            "The same company says the card on file expires before the renewal",
            ACTION,
        )
        .asks(vec![
            Item::asks(AskKind::FixPayment, "Cloudbox").due(Due::Stated)
        ])
        .twin_of("renewal-notice", "an action is needed before a date"),
    );
}

/// A request to sign against the completed document.
fn sign_request_or_completed(b: &mut Builder) {
    let docusign = addr("Marlow Ltd via DocuSign", "dse@docusign.net");
    let request = b.inbound(Mail::new(
        "p20-request",
        docusign.clone(),
        "Please sign: NDA with Marlow Ltd",
        "Marlow Ltd sent you a document to review and sign: NDA.\n\nREVIEW DOCUMENT https://www.docusign.net/Signing/?ti=p20",
        22,
    ));
    b.label(
        &request,
        Scenario::new(
            "sign-request",
            "A request to review and sign an NDA",
            ACTION,
        )
        .asks(vec![Item::asks(AskKind::Sign, "Marlow Ltd")]),
    );
    let done = b.inbound(Mail::new(
        "p20-completed",
        docusign,
        "Completed: NDA with Marlow Ltd",
        "All parties have signed the NDA. A copy is attached for your records.",
        10,
    ));
    b.label(
        &done,
        Scenario::new("sign-completed", "The same document, signed by everyone", NOTICE)
            .twin_of("sign-request", "already signed; nothing left to do")
            .review(
            &["record"],
            "A fully signed contract could be a record worth keeping; labelled no record because records are receipts, orders, bookings and bills today."),
    );
}

/// A friend's forward against the publisher's issue.
fn forwarded_newsletter_or_publisher(b: &mut Builder) {
    let forward = b.inbound(Mail::new(
        "p21-forward",
        addr("Gabe Lindqvist", "gabe@lindqvist.example"),
        "Fwd: The case for slow software",
        "Thought of you, worth a read.\n\n---------- Forwarded message ----------\nFrom: Slow Software Weekly\nSubject: The case for slow software\n\nSoftware that waits is software that listens.",
        25,
    ));
    b.label(
        &forward,
        Scenario::new(
            "friend-forwards-an-article",
            "A friend forwards an article with a note",
            CONVERSATION,
        ),
    );
    let issue = b.inbound(
        Mail::new(
            "p21-issue",
            addr("Slow Software Weekly", "issue@slowsoftware.example"),
            "The case for slow software",
            "Software that waits is software that listens. This week we look at why.",
            26,
        )
        .unsubscribe(),
    );
    b.label(
        &issue,
        Scenario::new(
            "publisher-sends-the-issue",
            "The publisher's own issue of the same article",
            READING,
        )
        .twin_of(
            "friend-forwards-an-article",
            "sent by the publisher, not a friend",
        ),
    );
}

/// A booking confirmation against a come-back-and-book nudge.
fn booking_or_booking_reminder_marketing(b: &mut Builder) {
    let (check_in, check_out) = (b.on(20), b.on(22));
    let booked = b.inbound(Mail::new(
        "p22-booked",
        addr("Marlow Inn", "reservations@marlowinn.example"),
        "Your booking at Marlow Inn is confirmed",
        &format!("Booking reference: MI-7712\nCheck-in: {check_in}\nCheck-out: {check_out}\nTotal £240.00"),
        27,
    ));
    b.label(
        &booked,
        Scenario::new("booking-confirmed", "A hotel confirms your booking", FILED),
    );
    let nudge = b.inbound(
        Mail::new(
            "p22-nudge",
            addr("Marlow Inn", "hello@marlowinn.example"),
            "Still thinking about Marlow Inn?",
            "Hi Alex,\n\nComplete your booking and get 10% off your first stay.",
            24,
        )
        .unsubscribe(),
    );
    b.label(
        &nudge,
        Scenario::new(
            "booking-nudge",
            "The same hotel nudges you to book, with List-Unsubscribe",
            READING,
        )
        .twin_of("booking-confirmed", "no booking exists; it is marketing"),
    );
}

/// A message to you alone against the same sender's broadcast to a crowd.
fn direct_or_crowd(b: &mut Builder) {
    let hana = addr("Hana Ito", "hana@fieldwork.example");
    let direct = b.inbound(Mail::new(
        "p23-direct",
        hana.clone(),
        "Desk move on Friday",
        "Hi Alex,\n\nI'm moving your desk to the third floor on Friday. Is that OK?\n\nHana",
        23,
    ));
    b.label(
        &direct,
        Scenario::new(
            "note-to-you-alone",
            "A colleague asks you about your desk",
            QUESTION,
        ),
    );
    let mut crowd: Vec<Address> = vec![b.me.clone()];
    crowd.extend((0..11).map(|i| {
        addr(
            &format!("Colleague {i}"),
            &format!("colleague{i}@fieldwork.example"),
        )
    }));
    let broadcast = b.inbound(
        Mail::new(
            "p23-crowd",
            hana,
            "Office closed Friday afternoon",
            "Hi all,\n\nThe office closes at 2pm on Friday for the fire drill.\n\nHana",
            22,
        )
        .to(crowd),
    );
    b.label(
        &broadcast,
        Scenario::new(
            "note-to-a-crowd",
            "The same colleague tells twelve people the office closes early",
            COPIED,
        )
        .twin_of("note-to-you-alone", "sent to twelve people, not to you"),
    );
}

/// The same cold email in the inbox and in the provider's spam folder.
fn inbox_or_spam_folder(b: &mut Builder) {
    let rory = addr("Rory Vance", "rory@vanceco.example");
    let inbox = b.inbound(Mail::new(
        "p24-inbox",
        rory.clone(),
        "Quick question about your CLI",
        "Hi Alex,\n\nI saw your talk. Do you have 15 minutes next week?\n\nRory",
        28,
    ));
    b.label(
        &inbox,
        Scenario::new(
            "cold-intro-in-inbox",
            "A stranger's short intro in the inbox",
            QUESTION,
        ),
    );
    let spam = b.inbound(
        Mail::new(
            "p24-spam",
            rory,
            "Quick question about your CLI (2)",
            "Hi Alex,\n\nI saw your talk. Do you have 15 minutes next week?\n\nRory",
            27,
        )
        .spam(),
    );
    b.label(
        &spam,
        Scenario::new(
            "cold-intro-in-spam",
            "The same intro, which the provider filed under Spam",
            expect(Who::Person, Home::Spam, false),
        )
        .twin_of("cold-intro-in-inbox", "the provider's spam verdict"),
    );
}

#[cfg(test)]
mod tests {
    use super::super::SELF_EMAIL;
    use super::*;

    #[test]
    fn the_account_owner_is_a_labelled_sender_only_for_a_promise() {
        let account_id = AccountId::from_provider_id("fake", SELF_EMAIL);
        let twins = build(&account_id, &super::super::self_address(), Utc::now(), 1);
        assert!(twins
            .labels
            .iter()
            .all(|label| label.outbound == label.from_email.eq_ignore_ascii_case(SELF_EMAIL)));
        assert!(
            twins.messages.len() > twins.labels.len(),
            "sent context exists"
        );
    }
}
