//! Twins for every shape chapter 37 lists (R1c): pairs of emails that differ
//! in the one fact that changes what they ask of the owner.
//!
//! Labels state what blueprint 37 says should happen (its answer table and
//! document-reading table), not what today's code does, so the pairs that
//! today's rules get wrong show up as misses. Senders, amounts and people are
//! invented. Every pair has its own people and sender, so one pair's history
//! never leaks into another's.

use super::twins::{
    addr, expect, Builder, Mail, Scenario, ACTION, CONVERSATION, COPIED, FILED, NOTICE, QUESTION,
    READING,
};
use super::{AskKind, Due, Home, Item, Who};

pub(super) fn build(b: &mut Builder) {
    marketing_with_pay_words_or_invoice(b);
    direct_debit_offer_or_notice(b);
    decline_resolved_or_current(b);
    credit_note_or_balance_due(b);
    final_reminder_with_or_without_money(b);
    platform_announcement_or_personal_invitation(b);
    owner_request_or_owner_promise(b);
    forwarded_letter_or_direct_request(b);
    two_accounts_from_one_sender(b);
    pound_euro_and_rand(b);
    group_cc_or_alone(b);
    conditional_or_unconditional(b);
    pleasantry_or_real_question(b);
    question_from_a_no_reply_address(b);
    answered_or_open(b);
    south_african_phrasings(b);
}

fn marketing_with_pay_words_or_invoice(b: &mut Builder) {
    let by = b.on(3);
    let promo = b.inbound(
        Mail::new(
            "a01-promo",
            addr("Skyline Hosting Offers", "offers@skylinehosting.example"),
            "Pay by Friday and lock in this year's price",
            &format!("Hi Alex,\n\nPay by {by} and keep this year's price: £120.00 instead of £150.00. Pay now: https://skylinehosting.example/offer\n\nOffer ends {by}."),
            9,
        )
        .unsubscribe(),
    );
    b.label(
        &promo,
        Scenario::new(
            "promo-with-pay-words",
            "A hosting company's promotion that says pay by a date and pay now, with an amount",
            READING,
        ),
    );
    let due = b.on(6);
    let invoice = b.inbound(Mail::new(
        "a01-invoice",
        addr("Skyline Hosting", "accounts@skylinehosting.example"),
        "Invoice SH-5107 for £120.00",
        &format!("Hi Alex,\n\nInvoice number: SH-5107\nAmount due: £120.00\nDue date: {due}\nPay online: https://skylinehosting.example/pay/SH-5107"),
        8,
    ));
    b.label(
        &invoice,
        Scenario::new(
            "invoice-from-the-same-company",
            "The same company's invoice, with an amount due and a due date",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Skyline Hosting")
            .amount(12_000, "GBP")
            .due(Due::AfterSent(152))
            .reference("SH-5107")])
        .twin_of(
            "promo-with-pay-words",
            "an invoice with an amount due, not an offer",
        ),
    );
}

fn direct_debit_offer_or_notice(b: &mut Builder) {
    let due = b.on(8);
    let offer = b.inbound(Mail::new(
        "a02-offer",
        addr("Northwater", "billing@northwater.example"),
        "Your water bill is ready",
        &format!("Hi Alex,\n\nYour bill of £58.30 is due on {due}.\nAccount number: NW-20817\nPay online: https://northwater.example/pay\n\nWant one thing less to remember? Set up a Direct Debit: https://northwater.example/direct-debit"),
        12,
    ));
    b.label(
        &offer,
        Scenario::new(
            "bill-with-a-direct-debit-offer",
            "A water bill that is still to be paid and offers to set up a Direct Debit",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Northwater")
            .amount(5_830, "GBP")
            .due(Due::AfterSent(204))
            .reference("NW-20817")])
        .review(
            &["record"],
            "A bill with an amount and a due date is a billing document worth keeping, the same call as todo-bill-with-due-date.",
        ),
    );
    let collected = b.on(8);
    let notice = b.inbound(Mail::new(
        "a02-notice",
        addr("Northwater", "billing@northwater.example"),
        "Your water bill is ready",
        &format!("Hi Alex,\n\nYour bill of £58.30 will be collected by Direct Debit on {collected}.\nAccount number: NW-20955\nYou don't need to do anything."),
        11,
    ));
    b.label(
        &notice,
        Scenario::new(
            "direct-debit-notice",
            "The same company's bill that it will collect by Direct Debit",
            FILED,
        )
        .twin_of(
            "bill-with-a-direct-debit-offer",
            "collected by Direct Debit, so nothing is owed by you",
        ),
    );
}

fn decline_resolved_or_current(b: &mut Builder) {
    let earlier = b.on(-20);
    let current = b.inbound(Mail::new(
        "a03-current",
        addr("Pulse Fit", "billing@pulsefit.example"),
        "Your payment was declined",
        "Hi Alex,\n\nYour card was declined when we tried to take your £34.00 membership payment. Update your card: https://pulsefit.example/billing",
        10,
    ));
    b.label(
        &current,
        Scenario::new("current-decline", "A gym's card was declined today", ACTION).asks(vec![
            Item::asks(AskKind::FixPayment, "Pulse Fit").amount(3_400, "GBP"),
        ]),
    );
    let resolved = b.inbound(Mail::new(
        "a03-resolved",
        addr("Pulse Fit", "billing@pulsefit.example"),
        "Your membership is in good standing",
        &format!("Hi Alex,\n\nYour payment of £34.00 was declined on {earlier}, but you updated your card and we have since taken it. Your membership is in good standing."),
        9,
    ));
    b.label(
        &resolved,
        Scenario::new(
            "past-decline-resolved",
            "The same gym says an earlier decline was resolved",
            NOTICE,
        )
        .twin_of(
            "current-decline",
            "the decline is narrated as already resolved",
        ),
    );
}

fn credit_note_or_balance_due(b: &mut Builder) {
    let credit = b.inbound(Mail::new(
        "a04-credit",
        addr("Harbour Lights Energy", "accounts@harbourlights.example"),
        "Credit note CN-118",
        "Hi Alex,\n\nCredit note CN-118\nCredit issued: £35.00\nAmount due: £0.00\nYour account is in credit; there is nothing to pay.",
        14,
    ));
    b.label(
        &credit,
        Scenario::new(
            "credit-note-nothing-due",
            "A credit note with £0.00 due",
            NOTICE,
        )
        .review(
            &["record"],
            "A credit note could be kept as a billing document; the corpus files receipts, orders, bookings and bills, so it is labelled no record.",
        ),
    );
    let due = b.on(7);
    let balance = b.inbound(Mail::new(
        "a04-balance",
        addr("Harbour Lights Energy", "billing@harbourlights.example"),
        "Your balance",
        &format!("Hi Alex,\n\nBill ST-4471\nAmount due: £210.50\nDue date: {due}\nPay online: https://harbourlights.example/pay"),
        13,
    ));
    b.label(
        &balance,
        Scenario::new(
            "positive-balance-due",
            "The same company's bill with a positive balance and a due date (a bill, not a statement: chapter 37 makes a statement a to-do only with a minimum payment and a date)",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Harbour Lights Energy")
            .amount(21_050, "GBP")
            .due(Due::AfterSent(181))
            .reference("ST-4471")])
        .twin_of("credit-note-nothing-due", "a balance above zero is due"),
    );
}

fn final_reminder_with_or_without_money(b: &mut Builder) {
    let ends = b.on(2);
    let sale = b.inbound(
        Mail::new(
            "a05-sale",
            addr("Gearloft Deals", "deals@gearloft.example"),
            "Final reminder: 20% off ends soon",
            &format!("Final reminder: our sale ends on {ends}. Use code AUTUMN20 at checkout."),
            16,
        )
        .unsubscribe(),
    );
    b.label(
        &sale,
        Scenario::new(
            "final-reminder-sale",
            "A shop's final reminder that a sale ends",
            READING,
        ),
    );
    let was_due = b.on(-4);
    let owed = b.inbound(Mail::new(
        "a05-owed",
        addr("Gearloft", "billing@gearloft.example"),
        "Final reminder: £64.10 overdue",
        &format!("Final reminder: invoice GL-3321 for £64.10 was due on {was_due}. Pay now to avoid a late fee: https://gearloft.example/pay"),
        15,
    ));
    b.label(
        &owed,
        Scenario::new(
            "final-reminder-money-owed",
            "The same shop's final reminder about an overdue invoice",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Gearloft")
            .amount(6_410, "GBP")
            .due(Due::AfterSent(-81))
            .reference("GL-3321")])
        .twin_of(
            "final-reminder-sale",
            "money is owed, not an offer that ends",
        ),
    );
}

fn platform_announcement_or_personal_invitation(b: &mut Builder) {
    let on = b.on(10);
    let platform = b.inbound(
        Mail::new(
            "a06-platform",
            addr("PlatformHQ Events", "events@events.platformhq.example"),
            "Join our autumn product webinar",
            &format!("Hi Alex,\n\nJoin us for our autumn product webinar on {on}. RSVP here: https://events.platformhq.example/rsvp"),
            19,
        )
        .unsubscribe(),
    );
    b.label(
        &platform,
        Scenario::new(
            "platform-announcement-with-rsvp",
            "A platform announces a webinar and says RSVP",
            READING,
        ),
    );
    let dinner = b.on(5);
    let personal = b.inbound(Mail::new(
        "a06-personal",
        addr("Mara Fielding", "mara@fieldhouse.example"),
        "Dinner at ours",
        &format!("Hi Alex,\n\nWe're having a few friends over for dinner on {dinner}. Please RSVP by Sunday so I know how much to cook.\n\nMara"),
        18,
    ));
    b.label(
        &personal,
        Scenario::new(
            "personal-invitation",
            "A friend invites you to dinner and asks for an RSVP by Sunday",
            CONVERSATION,
        )
        .asks(vec![
            Item::asks(AskKind::Attend, "Mara Fielding").due(Due::Words("Sunday"))
        ])
        .twin_of(
            "platform-announcement-with-rsvp",
            "a person invites you, not a platform announcing an event",
        ),
    );
}

fn owner_request_or_owner_promise(b: &mut Builder) {
    let dev = addr("Devika Rao", "devika@raoandco.example");
    let request = b.sent(
        "a07-request",
        dev.clone(),
        "Deck for Friday",
        "Hi Devika,\n\nCould you send me the deck by Friday?\n\nAlex",
        30,
    );
    b.label(
        &request,
        Scenario::new(
            "owner-asks-for-something",
            "You ask Devika to send you a deck by Friday",
            NOTICE,
        )
        .promises(Vec::new()),
    );
    let promise = b.sent(
        "a07-promise",
        dev,
        "Signed form",
        "Hi Devika,\n\nI'll send you the signed form by Friday.\n\nAlex",
        29,
    );
    b.label(
        &promise,
        Scenario::new(
            "owner-promises-something",
            "You tell Devika you will send her the signed form by Friday",
            NOTICE,
        )
        .promises(vec![
            Item::promised(AskKind::Send, "Devika Rao").due(Due::Words("Friday"))
        ])
        .twin_of(
            "owner-asks-for-something",
            "you promise something, instead of asking",
        ),
    );
}

fn forwarded_letter_or_direct_request(b: &mut Builder) {
    let due = b.on(12);
    let forwarded = b.inbound(Mail::new(
        "a08-forwarded",
        addr("Ola Bakare", "ola@bakare.example"),
        "Fwd: Parking charge notice",
        &format!("FYI, this came to my old address.\n\n---------- Forwarded message ----------\nFrom: Eastport Parking <notices@eastportparking.example>\nSubject: Parking charge notice\n\nDear Mr Bakare,\n\nYou owe a parking charge of £60.00. Pay by {due}: https://eastportparking.example/pay"),
        21,
    ));
    b.label(
        &forwarded,
        Scenario::new(
            "forwarded-letter-to-someone-else",
            "A friend forwards a parking charge letter addressed to him, saying it is for information",
            CONVERSATION,
        )
        .review(
            &["todo"],
            "The forwarded letter is addressed to the forwarder, not to you, and the forwarder asks nothing, so it asks nothing of you. If the forwarder had asked you to deal with it, it would be a pay item.",
        ),
    );
    let direct = b.inbound(Mail::new(
        "a08-direct",
        addr("Eastport Parking", "notices@eastportparking.example"),
        "Parking charge notice",
        &format!("Dear Alex,\n\nYou owe a parking charge of £60.00. Pay by {due}: https://eastportparking.example/pay"),
        20,
    ));
    b.label(
        &direct,
        Scenario::new(
            "direct-parking-charge",
            "The same notice sent to you directly",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Eastport Parking")
            .amount(6_000, "GBP")
            .due(Due::AfterSent(308))])
        .twin_of(
            "forwarded-letter-to-someone-else",
            "sent to you directly, not forwarded",
        )
        .review(
            &["record"],
            "A charge notice is a billing document worth keeping, the same call as a bill.",
        ),
    );
}

fn two_accounts_from_one_sender(b: &mut Builder) {
    let due = b.on(10);
    let sender = addr(
        "Ashgrove Property Management",
        "levies@ashgrove-managing.example",
    );
    let first = b.inbound(Mail::new(
        "a09-first",
        sender.clone(),
        "Service charge: Flat 2",
        &format!("Hi Alex,\n\nService charge for Flat 2, account AG-1001\nAmount due: £250.00\nDue date: {due}\nPay online: https://ashgrove-managing.example/pay"),
        24,
    ));
    b.label(
        &first,
        Scenario::new(
            "service-charge-first-account",
            "A managing agent's service charge for the first of your two flats",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(
            AskKind::Pay,
            "Ashgrove Property Management",
        )
        .amount(25_000, "GBP")
        .due(Due::AfterSent(264))
        .reference("AG-1001")])
        .review(
            &["record"],
            "A service charge demand is a billing document worth keeping, the same call as a bill.",
        ),
    );
    let second = b.inbound(Mail::new(
        "a09-second",
        sender,
        "Service charge: Flat 4",
        &format!("Hi Alex,\n\nService charge for Flat 4, account AG-1002\nAmount due: £250.00\nDue date: {due}\nPay online: https://ashgrove-managing.example/pay"),
        23,
    ));
    b.label(
        &second,
        Scenario::new(
            "service-charge-second-account",
            "The same agent's service charge for the second flat, same amount and due date",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(
            AskKind::Pay,
            "Ashgrove Property Management",
        )
        .amount(25_000, "GBP")
        .due(Due::AfterSent(263))
        .reference("AG-1002")])
        .twin_of(
            "service-charge-first-account",
            "a second account number, so a second thing to pay",
        )
        .review(
            &["record"],
            "A service charge demand is a billing document worth keeping, the same call as a bill.",
        ),
    );
}

fn pound_euro_and_rand(b: &mut Builder) {
    let due = b.on(9);
    let pound = b.inbound(Mail::new(
        "a10-pound",
        addr("Lumen Power", "accounts@lumenpower.example"),
        "Invoice LP-2201",
        &format!("Hi Alex,\n\nInvoice number: LP-2201\nAmount due: £1,234.56\nDue date: {due}\nPay online: https://lumenpower.example/pay"),
        26,
    ));
    b.label(
        &pound,
        Scenario::new(
            "invoice-in-pounds",
            "An invoice for £1,234.56",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Lumen Power")
            .amount(123_456, "GBP")
            .due(Due::AfterSent(242))
            .reference("LP-2201")])
        .review(
            &["record"],
            "An invoice is a billing document worth keeping, the same call as a bill.",
        ),
    );
    let euro = b.inbound(Mail::new(
        "a10-euro",
        addr("Lumiere Energie", "facturation@lumiere-energie.example"),
        "Invoice LE-2202",
        &format!("Hello Alex,\n\nInvoice number: LE-2202\nAmount due: €1.234,56\nDue date: {due}\nPay online: https://lumiere-energie.example/pay"),
        25,
    ));
    b.label(
        &euro,
        Scenario::new(
            "invoice-in-euros",
            "An invoice for €1.234,56 written with a decimal comma",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Lumiere Energie")
            .amount(123_456, "EUR")
            .due(Due::AfterSent(241))
            .reference("LE-2202")])
        .twin_of("invoice-in-pounds", "the amount is in euros")
        .review(
            &["record"],
            "An invoice is a billing document worth keeping, the same call as a bill.",
        ),
    );
    let rand = b.inbound(Mail::new(
        "a10-rand",
        addr("Kloof Power Supply", "accounts@kloofpower.example"),
        "Invoice KP-2203",
        &format!("Hello Alex,\n\nInvoice number: KP-2203\nAmount due: R1 234,56\nDue date: {due}\nPay online: https://kloofpower.example/pay"),
        24,
    ));
    b.label(
        &rand,
        Scenario::new(
            "invoice-in-rand",
            "An invoice for R1 234,56 written the South African way",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Kloof Power Supply")
            .amount(123_456, "ZAR")
            .due(Due::AfterSent(240))
            .reference("KP-2203")])
        .locale("en-ZA")
        .twin_of("invoice-in-pounds", "the amount is in rand")
        .review(
            &["record"],
            "An invoice is a billing document worth keeping, the same call as a bill.",
        ),
    );
}

fn group_cc_or_alone(b: &mut Builder) {
    let theo = addr("Theo Marsh", "theo@marshworks.example");
    let alone = b.inbound(Mail::new(
        "a11-alone",
        theo.clone(),
        "Meeting room on Tuesday",
        "Alex, could you book the meeting room for Tuesday?\n\nTheo",
        28,
    ));
    b.label(
        &alone,
        Scenario::new(
            "ask-to-you-alone",
            "A colleague asks you by name to book a meeting room",
            CONVERSATION,
        )
        .asks(vec![Item::asks(AskKind::Book, "Theo Marsh")]),
    );
    let group = b.inbound(
        Mail::new(
            "a11-group",
            theo.clone(),
            "Meeting room on Tuesday, anyone?",
            "Hi all,\n\nCould someone book the meeting room for Tuesday?\n\nTheo",
            27,
        )
        .to(vec![
            b.me.clone(),
            addr("Ines Calder", "ines@marshworks.example"),
            addr("Kofi Mensah", "kofi@marshworks.example"),
        ]),
    );
    b.label(
        &group,
        Scenario::new(
            "ask-to-a-group",
            "The same colleague asks three people if someone will book the room",
            CONVERSATION,
        )
        .twin_of("ask-to-you-alone", "asked of a group, no one by name")
        .review(
            &["todo"],
            "\"Could someone book\" names nobody, so the actor is not resolved to you and it is labelled as asking nothing of you; a model could reasonably call it a shared ask.",
        ),
    );
    let copied = b.inbound(
        Mail::new(
            "a11-cc",
            theo,
            "Meeting room on Tuesday, for Ines",
            "Ines, could you book the meeting room for Tuesday? Alex, copying you for visibility.\n\nTheo",
            26,
        )
        .to(vec![addr("Ines Calder", "ines@marshworks.example")])
        .cc(vec![b.me.clone()]),
    );
    b.label(
        &copied,
        Scenario::new(
            "ask-to-someone-else-you-are-copied-on",
            "The same colleague asks Ines and only copies you",
            COPIED,
        )
        .twin_of(
            "ask-to-you-alone",
            "asked of someone else, you are only copied",
        ),
    );
}

fn conditional_or_unconditional(b: &mut Builder) {
    let naledi = addr("Naledi Dube", "naledi@dubeandco.example");
    let plain = b.inbound(Mail::new(
        "a12-plain",
        naledi.clone(),
        "Your CV",
        "Hi Alex,\n\nPlease send me your CV by Friday, we are shortlisting then.\n\nNaledi",
        32,
    ));
    b.label(
        &plain,
        Scenario::new(
            "unconditional-ask",
            "A recruiter asks you to send your CV by Friday",
            CONVERSATION,
        )
        .asks(vec![Item::asks(AskKind::Send, "Naledi Dube")
            .due(Due::Words("Friday"))
            .provenance(
                "Please send me your CV by Friday, we are shortlisting then.",
                "owner",
                "pending",
            )]),
    );
    let conditional = b.inbound(Mail::new(
        "a12-conditional",
        addr("Lerato Dube", "lerato@dubeandco.example"),
        "About the role",
        "Hi Alex,\n\nIf you would like to be considered, you could send over your CV whenever suits you. No pressure either way.\n\nLerato",
        31,
    ));
    b.label(
        &conditional,
        Scenario::new(
            "conditional-ask",
            "A colleague of the recruiter makes the same request only if you would like to be considered",
            CONVERSATION,
        )
        .twin_of("unconditional-ask", "conditional on what you want")
        .review(
            &["todo"],
            "Chapter 37 names \"if you'd like to join, come\" as a conditional unit and says it asks nothing; labelled so.",
        ),
    );
}

fn pleasantry_or_real_question(b: &mut Builder) {
    let joel = addr("Joel Kent", "joel@kentpress.example");
    let pleasantry = b.inbound(Mail::new(
        "a13-pleasantry",
        joel.clone(),
        "Agenda for Thursday",
        "Hi Alex,\n\nAttached is the agenda for Thursday. Let me know if you have any questions.\n\nJoel",
        35,
    ));
    b.label(
        &pleasantry,
        Scenario::new(
            "let-me-know-if-you-have-questions",
            "A colleague sends the agenda and says to let him know if you have questions",
            CONVERSATION,
        ),
    );
    let question = b.inbound(Mail::new(
        "a13-question",
        addr("Joan Kent", "joan@kentpress.example"),
        "Which agenda",
        "Hi Alex,\n\nWhich of the two agendas works better for you, A or B?\n\nJoan",
        34,
    ));
    b.label(
        &question,
        Scenario::new(
            "real-question",
            "Another colleague asks which of two agendas you prefer",
            QUESTION,
        )
        .twin_of(
            "let-me-know-if-you-have-questions",
            "a real question, not a pleasantry",
        ),
    );
}

/// The same question, from an address that cannot be answered.
fn question_from_a_no_reply_address(b: &mut Builder) {
    let vendor = b.inbound(Mail::new(
        "a13-vendor",
        addr("Fernbank Supplies", "no-reply@vendor.example"),
        "Invoice reference",
        "Hi Alex,\n\nCould you confirm the invoice reference?\n\nFernbank Supplies",
        33,
    ));
    b.label(
        &vendor,
        Scenario::new(
            "no-reply-vendor-question",
            "A supplier's no-reply address asks you to confirm an invoice reference",
            NOTICE,
        )
        .twin_of("real-question", "from a no-reply address, not a person")
        .review(
            &["todo"],
            "A message nobody can answer is not Your turn: the desk needs a sender that classifies as a person (`requires_reply`).",
        ),
    );
}

fn answered_or_open(b: &mut Builder) {
    let open = b.inbound(Mail::new(
        "a14-open",
        addr("Kemi Arrow", "kemi@arrowdesign.example"),
        "Call on Thursday?",
        "Hi Alex,\n\nAre you free for a call on Thursday?\n\nKemi",
        6,
    ));
    b.label(
        &open,
        Scenario::new(
            "open-question",
            "A designer asks if you are free for a call, and you have not answered",
            QUESTION,
        ),
    );
    let ife = addr("Ife Arrow", "ife@arrowdesign.example");
    let asked = b.inbound(Mail::new(
        "a14-answered",
        ife.clone(),
        "Call on Friday?",
        "Hi Alex,\n\nAre you free for a call on Friday?\n\nIfe",
        6,
    ));
    b.label(
        &asked,
        Scenario::new(
            "question-answered-an-hour-later",
            "A second designer asks the same, and you answered an hour later",
            CONVERSATION,
        )
        .twin_of("open-question", "you answered an hour later")
        .review(
            &["todo"],
            "Your answer in the thread closes the question, so no reply is owed; the desk reads the thread, not the single email, to know that.",
        ),
    );
    b.sent(
        "a14-answered",
        ife,
        "Re: Call on Friday?",
        "Hi Ife,\n\nYes, Friday works. Shall we say 10am?\n\nAlex",
        5,
    );
}

fn south_african_phrasings(b: &mut Builder) {
    let due = b.on(11);
    let municipal = b.inbound(Mail::new(
        "a15-municipal",
        addr("Kloof Municipality", "accounts@kloof-municipal.example"),
        "Municipal account 4410 7731",
        &format!("Dear Ratepayer,\n\nYour municipal account 4410 7731 shows an amount due of R2 345,10. Please pay by {due} to avoid interest. Pay at any Kloof Municipality cash office or online."),
        40,
    ));
    b.label(
        &municipal,
        Scenario::new(
            "municipal-account-in-rand",
            "A municipality's rates and services account in rand",
            expect(Who::Machine, Home::Updates, true),
        )
        .asks(vec![Item::asks(AskKind::Pay, "Kloof Municipality")
            .amount(234_510, "ZAR")
            .due(Due::AfterSent(304))
            .reference("4410 7731")])
        .locale("en-ZA")
        .review(
            &["record"],
            "A municipal account is a billing document worth keeping, the same call as a bill.",
        ),
    );
    let eft = b.inbound(Mail::new(
        "a15-eft",
        addr("Thandi Sibiya", "thandi@sibiyaandco.example"),
        "Braai money",
        "Howzit Alex,\n\nCould you EFT the R450,00 for the braai to my account by Friday? Use BRAAI as the reference.\n\nThandi",
        38,
    ));
    b.label(
        &eft,
        Scenario::new(
            "friend-asks-for-an-eft",
            "A friend asks you to EFT R450,00 by Friday",
            CONVERSATION,
        )
        .asks(vec![Item::asks(AskKind::Pay, "Thandi Sibiya")
            .amount(45_000, "ZAR")
            .due(Due::Words("Friday"))])
        .locale("en-ZA"),
    );
}
