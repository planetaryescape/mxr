//! Held-out phrasings.
//!
//! DO NOT READ THIS FILE WHILE CHANGING A JUDGE OR A RULE. Agents that
//! improve how mxr sorts mail must not look at it, copy from it or tune
//! against it, and must not read the held-out numbers' causes off the text
//! here. Each family in `bulk/` has a few phrasings of its mail that the
//! eval grades separately (`mxr classify eval --set full` reports held-out
//! accuracy on its own line). If the held-out accuracy falls behind the
//! rest, the judge learned the generator, not the mail (blueprint 32). That
//! check only works while nobody has seen these phrasings.
//!
//! Every list here follows the same shape as the visible variants of the
//! family, in the same order of senders, so a held-out phrasing is the same
//! kind of mail worded differently. The expected answers are the family's
//! and live in `bulk/`; nothing here states one.
//!
//! The variants of a family are the visible ones first, then these.
//!
//! Whole template families are held out too ([`families`]): pairs of emails
//! whose wording and sender kind no other scenario shares, labelled
//! `held_out` so the eval reports them apart from the visible corpus.

use super::bulk::Variant;
use super::twins::{addr, expect, Builder, Mail, Scenario, ACTION, NOTICE};
use super::{AskKind, Due, Home, Item, Who};

/// The held-out phrasings of the family with this id.
pub(crate) fn variants(family_id: &str) -> &'static [Variant] {
    match family_id {
        "people-thread" => PEOPLE_THREAD,
        "dated-request" => DATED_REQUEST,
        "hiring-thread" => HIRING_THREAD,
        "copied-thread" => COPIED_THREAD,
        "order-receipt" => ORDER_RECEIPT,
        "bill-due" => BILL_DUE,
        "travel-booking" => TRAVEL_BOOKING,
        "service-alert" => SERVICE_ALERT,
        "security-notice" => SECURITY_NOTICE,
        "newsletter" => NEWSLETTER,
        "promotion" => PROMOTION,
        "spam-prize" => SPAM_PRIZE,
        "phishing-lookalike" => PHISHING,
        _ => &[],
    }
}

const PEOPLE_THREAD: &[Variant] = &[
    Variant {
        id: "formal",
        subjects: &[],
        bodies: &[
            "Dear Alex,\n\nFurther to our discussion regarding {topic}, I would be grateful if you could confirm the current position and let me know of any concerns before we proceed. I am conscious of the wider timetable and would like to avoid any avoidable delay.\n\nKind regards,\n{name}",
            "Thank you for your reply. I have noted your points and will circulate an updated position to the group in due course.\n\nKind regards,\n{name}",
            "Thank you once more. I consider the matter closed unless anything further arises.\n\nKind regards,\n{name}",
        ],
    },
    Variant {
        id: "forwarded-context",
        subjects: &[],
        bodies: &[
            "Hi Alex,\n\nForwarding the thread I mentioned so you have the background on {topic}. The short version: the team wants a decision by the end of the month and I'd like us to speak with one voice. Have a read and tell me where you differ.\n\n{name}",
            "Thanks for reading it through. Your point about the reporting gap is fair; I'll raise it with {other} and come back to you.\n\n{name}",
            "{other} agrees with you, so we're aligned. Thanks for the steer.\n\n{name}",
        ],
    },
];

const DATED_REQUEST: &[Variant] = &[
    Variant {
        id: "draft-contract",
        subjects: &["Draft contract comments"],
        bodies: &[
            "Hi Alex,\n\nI need your comments on the draft contract back on {due}, since legal wants to send it that evening. A marked-up copy is fine.\n\n{name}",
            "Thanks, I'll leave room in the afternoon to fold them in.\n\n{name}",
            "Your comments are in and legal has the draft. Thank you.\n\n{name}",
        ],
    },
    Variant {
        id: "parking-permit",
        subjects: &["Parking permit renewal"],
        bodies: &[
            "Alex, your office parking permit runs out soon. Fill in the renewal form and return it to me ahead of {due}.\n\n{name}",
            "Thanks, I'll keep the form on my desk for you.\n\n{name}",
            "Form received, your permit is renewed.\n\n{name}",
        ],
    },
];

const HIRING_THREAD: &[Variant] = &[
    Variant {
        id: "referral",
        subjects: &["Referral: senior SRE"],
        bodies: &[
            "Hi Alex,\n\nA friend of the team referred a senior SRE and I've attached the screen notes. Would you be willing to join the technical conversation?\n\n{name}",
            "Wonderful, I'll find a slot that suits you and send an invite.\n\n{name}",
        ],
    },
    Variant {
        id: "panel-debrief",
        subjects: &["Panel debrief: engineering manager"],
        bodies: &[
            "Alex,\n\nDebrief notes from the engineering manager panel are attached. Opinions are split on leadership experience. I'd like to hear yours before we meet.\n\n{name}",
            "Thanks, that tips it for me. We'll discuss at the hiring meeting and I'll report back.\n\n{name}",
        ],
    },
];

const COPIED_THREAD: &[Variant] = &[
    Variant {
        id: "invoice-query",
        subjects: &[],
        bodies: &[
            "Hi {other},\n\nThe supplier invoice for {topic} doesn't match the purchase order. I've put Alex on copy as the budget holder.\n\n{name}",
            "Thanks for catching it. I'll ask them for a corrected invoice.\n\n{name}",
            "They've sent a corrected one. The totals match now.\n\n{name}",
            "Good. I'll pass it for payment.\n\n{name}",
            "Done, thank you.\n\n{name}",
        ],
    },
    Variant {
        id: "launch-timing",
        subjects: &[],
        bodies: &[
            "Hi {other},\n\nOn {topic}, do you think we can hold the launch date? I've copied Alex so they see where we are.\n\n{name}",
            "I think so, provided the last two fixes land by midweek.\n\n{name}",
            "That's tight but possible. I'll check with the team.\n\n{name}",
            "They're confident. We keep the date.\n\n{name}",
            "Great news. I'll update the plan.\n\n{name}",
        ],
    },
];

const ORDER_RECEIPT: &[Variant] = &[
    Variant {
        id: "formal-invoice-first",
        subjects: &[
            "Sales order acknowledgement {ref}",
            "Despatch advice {ref}",
            "Goods received confirmation {ref}",
            "Service feedback request {ref}",
            "Sales invoice {ref}",
        ],
        bodies: &[
            "Sales order acknowledgement\n\nOrder reference: {ref}\nOrder date: {date}\nValue: {amount}\n\nThank you for your custom. We will despatch your goods shortly.\n\n{org}",
            "Despatch advice for order {ref}. Your goods have left our warehouse and are expected within three working days.\n\n{org}",
            "We confirm that the goods on order {ref} have been delivered to your address.\n\n{org}",
            "We would be grateful for your feedback on order {ref}. The survey takes less than a minute.\n\n{org}",
            "Sales invoice for order {ref}\nIssued: {date}\nTotal due: {amount} (paid)\n\n{org}",
        ],
    },
    Variant {
        id: "marketing-heavy",
        subjects: &[
            "Thanks for your order! ({ref})",
            "Your goodies are on their way ({ref})",
            "Your goodies have landed ({ref})",
            "Tell us what you think ({ref})",
            "Your invoice for order {ref}",
        ],
        bodies: &[
            "Thank you, you're brilliant! Order {ref} is confirmed.\n\nTotal: {amount}\n\nWhile you wait, take 10% off your next order with the code THANKS10.\n\nLots of love,\n{org}",
            "Woo! Your order {ref} has shipped. Follow us for tips on getting the most from your new kit.\n\n{org}",
            "Your order {ref} has arrived. Show us how you're using it and you could win a gift card.\n\n{org}",
            "Loved it? Tell us in a review. Not so much? Tell us that too; we read every one.\n\n{org}",
            "Your invoice for order {ref} is below. Total {amount}. Thanks again for shopping with {org}.",
        ],
    },
];

const BILL_DUE: &[Variant] = &[
    Variant {
        id: "short",
        subjects: &[
            "Pay {amount} by {due}",
            "Paid: {amount}",
            "Statement: ready",
            "Pricing update",
            "One question",
        ],
        bodies: &[
            "Your payment of {amount} is due on {due}. Pay here: https://billing.demo.mxr.local/pay/{ref}\n\n{org}",
            "Payment of {amount} received. Thank you.\n\n{org}",
            "Your statement is ready to view online.\n\n{org}",
            "Some of our prices change next month. No action needed.\n\n{org}",
            "How likely are you to recommend us to a friend? Answer in one click.\n\n{org}",
        ],
    },
    Variant {
        id: "collections-polite",
        subjects: &[
            "Your account {ref} needs a payment",
            "We've received your payment",
            "Your account summary",
            "Updates to our terms and rates",
            "Share your thoughts",
        ],
        bodies: &[
            "Hello Alex,\n\nWe're writing about your account {ref}. A payment of {amount} is outstanding, due by {due}. Pay securely here: https://billing.demo.mxr.local/pay/{ref}\n\nIf you've already paid, thank you, and please ignore this email.\n\n{org}",
            "Thank you, Alex. Your payment of {amount} has been applied to account {ref}.\n\n{org}",
            "Your account summary is ready in your online account whenever you'd like to see it.\n\n{org}",
            "We're updating our terms and rates from next month. You don't need to do anything.\n\n{org}",
            "We'd love to hear what you think of us. It only takes a minute.\n\n{org}",
        ],
    },
];

const TRAVEL_BOOKING: &[Variant] = &[
    Variant {
        id: "hotel-boutique",
        subjects: &[
            "You're booked in ({ref})",
            "Thanks for your deposit ({ref})",
            "We can't wait to see you",
            "A small update to your arrival",
            "Tell us about your stay",
        ],
        bodies: &[
            "We're delighted to confirm your stay.\n\nBooking number: {ref}\nBooked: {date}\nRoom: Garden suite\n\nYour confirmation is attached.",
            "We've received your deposit of {amount} for booking {ref}. Thank you.",
            "Your stay with {org} is almost here. Our team is looking forward to meeting you.",
            "Our reception opens a little later on the day you arrive. Your room will be ready as planned.",
            "We hope you had a wonderful stay with {org}. Tell us about it in a short survey.",
        ],
    },
    Variant {
        id: "rail-flexi",
        subjects: &[
            "Booking {ref}: your flexi tickets",
            "Booking {ref}: payment confirmed",
            "Your trip: what to know",
            "Service alteration on your route",
            "Your trip: tell us how it went",
        ],
        bodies: &[
            "Your flexi tickets are issued.\n\nBooking: {ref}\nIssued: {date}\nPassengers: 1\n\nYour itinerary is attached.",
            "We have taken payment of {amount} for booking {ref}.",
            "Your trip is coming up. Seat reservations and ticket details are in your account.",
            "A planned engineering work changes the timetable on part of your route. Your tickets remain valid.",
            "Tell us how your trip went. It takes under a minute.",
        ],
    },
];

const SERVICE_ALERT: &[Variant] = &[
    Variant {
        id: "nightly-warnings",
        subjects: &["Nightly job finished with warnings"],
        bodies: &["The nightly data job finished at 03:12 with {n} warnings and no errors. Output: https://ci.demo.mxr.local/jobs/{ref}\n\nBuild Watch"],
    },
    Variant {
        id: "response-slow",
        subjects: &["Response time slower than usual on api.demo.mxr.local"],
        bodies: &["Average response time for api.demo.mxr.local was {n}00 ms over the last hour, above the usual range. No downtime was recorded.\n\nUptime Robot"],
    },
];

const SECURITY_NOTICE: &[Variant] = &[
    Variant {
        id: "new-browser",
        subjects: &["We noticed a sign-in from a new browser"],
        bodies: &["Someone signed in to your account from a new browser on {date}. If it was you, there's nothing to do.\n\nIf it wasn't, take a moment to review your account's security: https://accounts.demo.mxr.local/security/{ref}"],
    },
    Variant {
        id: "recovery-phone",
        subjects: &["Your recovery phone number changed"],
        bodies: &["The recovery phone number on your account was changed on {date}. We're letting you know so you're aware.\n\nDidn't make this change? See https://accounts.demo.mxr.local/security/{ref}"],
    },
];

const NEWSLETTER: &[Variant] = &[
    Variant {
        id: "notes-queries",
        subjects: &["SQLite Notes: ten queries you should know"],
        bodies: &["SQLite Notes\n\nTen queries that earn their keep: window functions, upserts, JSON helpers and a few plans worth reading closely.\n\nRead the full list: https://sqlite.org/lang.html\n\nUnsubscribe at the foot of this email."],
    },
    Variant {
        id: "ops-capacity",
        subjects: &["Ops Digest: capacity planning without a spreadsheet"],
        bodies: &["Ops Digest\n\nHow three teams plan capacity from a few graphs and a sensible buffer, plus a short reading list on queueing theory.\n\nhttps://lists.demo.mxr.local/ops/{n}\n\nStop receiving this digest."],
    },
];

const PROMOTION: &[Variant] = &[
    Variant {
        id: "fare-weekend",
        subjects: &["Weekend getaways from {amount}"],
        bodies: &["Fancy a weekend away? City breaks start from {amount} return, booked by {due}.\n\nBrowse getaways: https://promo.demo.mxr.local/offers/weekend\n\nUnsubscribe from fare emails."],
    },
    Variant {
        id: "conference-group",
        subjects: &["Bring your team: group passes now on sale"],
        bodies: &["Group passes for five or more are now on sale, with a discount that grows with the size of your team.\n\nSee group pricing: https://promo.demo.mxr.local/offers/group\n\nUnsubscribe from event emails."],
    },
];

const SPAM_PRIZE: &[Variant] = &[
    Variant {
        id: "verify-lottery",
        subjects: &["Mailbox verification needed to receive lottery funds"],
        bodies: &["Dear beneficiary,\n\nWe hold lottery funds in your name. To release them, verify your mailbox and send a copy of your ID.\n\nhttps://claim-now.demo.mxr.local/lottery\n\nAccount Verification Team"],
    },
    Variant {
        id: "prize-holiday",
        subjects: &["Free holiday for two, reply to confirm"],
        bodies: &["You have won a free holiday for two. Reply to this email with your name and phone number to confirm your place.\n\nPrize Desk"],
    },
];

const PHISHING: &[Variant] = &[
    Variant {
        id: "storage-full",
        subjects: &["Action required: your storage is almost full"],
        bodies: &["Your account storage is almost full and uploads will stop. Action required: upgrade or clear space by signing in here.\n\nhttps://security-mail.demo.mxr.local/storage\n\nAccount Verification"],
    },
    Variant {
        id: "winner-delivery",
        subjects: &["Urgent: confirm delivery of your prize"],
        bodies: &["Urgent password reset of your delivery profile is required before we can send your prize. Sign in with your usual details to continue.\n\nhttps://claim-now.demo.mxr.local/delivery\n\nPrize Desk"],
    },
];

/// The held-out template families: kinds of mail the visible corpus has no
/// phrasing for. Each is a pair, an ask and a near miss that asks nothing.
pub(super) fn families(b: &mut Builder) {
    school_consent_form(b);
    vet_balance_payable(b);
}

/// A school asks for a signed consent form; its newsletter does not.
fn school_consent_form(b: &mut Builder) {
    let trip = b.inbound(Mail::new(
        "h01-trip",
        addr("Elmwood Primary School", "office@elmwoodprimary.example"),
        "Year 4 geography trip",
        "Dear parent or carer,\n\nThe Year 4 trip to the coast runs next month. The consent form must be signed and handed to the class teacher by Friday, or your child cannot join the coach.\n\nMrs Adeyemi, school office",
        22,
    ));
    b.label(
        &trip,
        Scenario::new(
            "school-trip-consent-form",
            "A primary school needs a signed consent form handed in by Friday",
            expect(Who::Machine, Home::Updates, false),
        )
        .asks(vec![Item::asks(AskKind::Sign, "Elmwood Primary School").due(Due::Words("Friday"))])
        .held_out()
        .review(
            &["who", "home"],
            "A school office writes as an organisation with a person's signature; whether mxr calls it a person or a machine is a call, labelled machine because it is a form letter.",
        ),
    );
    let photos = b.inbound(Mail::new(
        "h01-photos",
        addr("Elmwood Primary School", "office@elmwoodprimary.example"),
        "Sports day photographs",
        "Dear parent or carer,\n\nThe photographs from sports day are now on the school gallery. Thank you to everyone who came along.\n\nMrs Adeyemi, school office",
        21,
    ));
    b.label(
        &photos,
        Scenario::new(
            "school-photos-notice",
            "The same school says the sports day photographs are online",
            NOTICE,
        )
        .held_out()
        .twin_of(
            "school-trip-consent-form",
            "photographs are online, nothing is asked",
        )
        .review(&["who", "home"], "Same call as school-trip-consent-form."),
    );
}

/// A vet's balance is "now payable"; its insurer-paid notice is not.
fn vet_balance_payable(b: &mut Builder) {
    let payable = b.inbound(Mail::new(
        "h02-payable",
        addr("Willowbank Vets", "reception@willowbankvets.example"),
        "Biscuit's treatment",
        "Hello Alex,\n\nBiscuit's treatment is complete. The remaining balance of £86.00 is now payable; please settle it at reception or by card over the phone within seven days.\n\nWillowbank Vets",
        20,
    ));
    b.label(
        &payable,
        Scenario::new(
            "vet-balance-now-payable",
            "A vet says the balance of £86.00 is now payable within seven days",
            ACTION,
        )
        .asks(vec![Item::asks(AskKind::Pay, "Willowbank Vets")
            .amount(8_600, "GBP")
            .due(Due::AfterSent(168))])
        .held_out(),
    );
    let insured = b.inbound(Mail::new(
        "h02-insured",
        addr("Willowbank Vets", "reception@willowbankvets.example"),
        "Biscuit's treatment, settled",
        "Hello Alex,\n\nBiscuit's treatment is complete and your pet insurer has paid the remaining balance of £86.00 directly to us. There is nothing to pay.\n\nWillowbank Vets",
        19,
    ));
    b.label(
        &insured,
        Scenario::new(
            "vet-balance-paid-by-insurer",
            "The same vet says the insurer has paid the balance",
            NOTICE,
        )
        .held_out()
        .twin_of(
            "vet-balance-now-payable",
            "the insurer has paid, so nothing is owed by you",
        ),
    );
}
