//! Families of mail between people: a colleague's thread with you, a dated
//! request, a hiring thread, and a thread you were only copied on.
//!
//! Visible phrasings only. The held-out ones live in `scenarios/held_out.rs`.

use super::expect;
use super::{
    Attachments, Dating, Family, Senders, Subjects, Turn, Unsubscribe, Variant, CONVERSATION,
    COPIED,
};
use super::{Home, Who};

const NO_SENT: &[&str] = &[];

/// Replies of yours, shared by the families you take part in. They are
/// context for the classifier (you have written to this person) and are
/// never labelled.
pub(super) const SENT_REPLIES: &[&str] = &[
    "Hi {name},\n\nThanks for this. I think we're on the same page; I'll pick it up tomorrow morning and come back to you.\n\nAlex",
    "Hi {name},\n\nGood question. My view is that we keep it as it is for now and revisit once we have more data. Happy to talk it through if that helps.\n\nAlex",
    "{name}, understood. I'll take the next step and let you know when it's done.\n\nAlex",
    "Thanks {name}. Sounds good on my side, go ahead.\n\nAlex",
    "Hi {name},\n\nThat works for me. I've added a couple of notes inline; shout if any of them don't sit right.\n\nAlex",
];

fn work_or_product(thread_num: usize) -> Option<&'static str> {
    Some(if thread_num % 13 == 4 {
        "product"
    } else {
        "work"
    })
}

fn work(_: usize) -> Option<&'static str> {
    Some("work")
}

fn hiring(_: usize) -> Option<&'static str> {
    Some("hiring")
}

/// The product threads carry a remote screenshot, so the reader's
/// remote-images control has something to block.
fn product_look(thread_num: usize) -> usize {
    if thread_num % 13 == 4 {
        4
    } else {
        0
    }
}

fn plain(_: usize) -> usize {
    0
}

fn hiring_look(_: usize) -> usize {
    8
}

pub(super) static PEOPLE_THREAD: Family = Family {
    id: "people-thread",
    senders: Senders::People { copied: false },
    subjects: Subjects::Pool,
    observer_cc: true,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "T",
    style: product_look,
    attachments: Attachments::None,
    brief_every_41: true,
    unsubscribe: Unsubscribe::None,
    provider_label: work_or_product,
    waiting: true,
    spam_folder: false,
    archived_every: None,
    turns: &[
        Turn::them(
            "bulk-people-opening",
            "A colleague opens a work thread with you and asks you something",
            CONVERSATION,
        ),
        Turn::you("bulk-people-your-reply", "Your reply"),
        Turn::them(
            "bulk-people-follow-up",
            "The colleague answers your reply and says what they will do next",
            CONVERSATION,
        ),
        Turn::you("bulk-people-your-second-reply", "Your short second reply"),
        Turn::them(
            "bulk-people-close",
            "The colleague closes the thread with thanks and asks nothing",
            CONVERSATION,
        ),
    ],
    sent_replies: SENT_REPLIES,
    variants: &[
        Variant {
            id: "terse",
            subjects: &[],
            bodies: &[
                "Hi Alex,\n\nQuick one on {topic}: where did we land on the owner? I'd like to close this out today.\n\n{name}",
                "Thanks, that works for me. I'll update the doc and ping you when it's ready.\n\n{name}",
                "All sorted on my side, thanks for the quick turnaround.\n\n{name}",
            ],
        },
        Variant {
            id: "friendly-long",
            subjects: &[],
            bodies: &[
                "Hi Alex,\n\nHope the week is going well. I've been going back through {topic} and had a few thoughts I wanted to run past you before we take it wider. The main one is whether we're being realistic about the timeline given how much is still open on the dependency side.\n\nNo rush, but I'd value your read when you have a moment.\n\nBest,\n{name}",
                "That's really helpful, thank you. I hadn't considered the migration order, and it changes what I'd propose to the group. I'll rework the plan around it and send you a revised version.\n\nBest,\n{name}",
                "Brilliant, that settles it for me. Thanks for making the time, and for the patience with the earlier drafts.\n\nBest,\n{name}",
            ],
        },
        Variant {
            id: "bullets",
            subjects: &[],
            bodies: &[
                "Hi Alex,\n\nThree things on {topic}:\n\n- the scope is still wider than we agreed\n- {other} is waiting on our answer\n- we need someone to own the rollout\n\nWhat's your view on the first two?\n\n{name}",
                "Agreed on the scope. I'll cut it back to what we listed and flag the rest as a later phase.\n\n{name}",
                "Good, that covers all three. Closing this out on my side.\n\n{name}",
            ],
        },
        Variant {
            id: "status-led",
            subjects: &[],
            bodies: &[
                "Alex,\n\nStatus on {topic}: the first pass is done and the numbers look reasonable. Two questions are left open, and both are yours: who signs off, and whether we announce before or after the review.\n\n{name}",
                "Understood. I'll hold the announcement until after the review and set up the sign-off with {other}.\n\n{name}",
                "Done. The sign-off is in and nothing else is open on this.\n\n{name}",
            ],
        },
        Variant {
            id: "question-led",
            subjects: &[],
            bodies: &[
                "Hi Alex,\n\nDo you remember why we dropped the second option on {topic}? I'm trying to explain it to someone who joined this week and can't find the reasoning anywhere.\n\nThanks,\n{name}",
                "That makes sense, I'd forgotten the cost angle. I'll write it up properly so it stops coming up.\n\nThanks,\n{name}",
                "Written up and saved with the other decisions. Thanks again.\n\n{name}",
            ],
        },
        Variant {
            id: "casual",
            subjects: &[],
            bodies: &[
                "hey Alex, saw your note on {topic}. made a few edits to the draft, mostly wording but I moved the risks section up front. have a look when you can and tell me if it reads better\n\n{name}",
                "glad it reads better. I'll tidy the last section and share it with {other} tomorrow\n\n{name}",
                "shared, and they're happy with it. thanks!\n\n{name}",
            ],
        },
    ],
};

pub(super) static DATED_REQUEST: Family = Family {
    id: "dated-request",
    senders: Senders::People { copied: false },
    subjects: Subjects::Thread,
    observer_cc: true,
    dating: Dating::Asks,
    due_days: 3,
    ref_prefix: "R",
    style: plain,
    attachments: Attachments::None,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: work,
    waiting: true,
    spam_folder: false,
    archived_every: None,
    turns: &[
        Turn::them(
            "bulk-dated-request-ask",
            "A colleague asks you to send something by a date",
            expect(Who::Person, Home::Messages, true, false),
        )
        .reviewed(
            &["todo"],
            "A colleague's request with a verb, an object and a date in their own words is an instruction, so it makes a to-do while the date is open (the landlord's lease in modes-landlord-reply is the same shape). Whether a person's ask belongs in To do or only in Your turn is a policy call. Mail whose date had passed before it was found makes no to-do, as todo-event-already-over says.",
        ),
        Turn::you("bulk-dated-request-your-reply", "Your reply"),
        Turn::them(
            "bulk-dated-request-acknowledged",
            "The colleague acknowledges your reply; nothing new is asked",
            CONVERSATION,
        ),
        Turn::you("bulk-dated-request-your-second-reply", "Your short second reply"),
        Turn::them(
            "bulk-dated-request-received",
            "The colleague confirms they have what they asked for",
            CONVERSATION,
        ),
    ],
    sent_replies: SENT_REPLIES,
    variants: &[
        Variant {
            id: "vendor-form",
            subjects: &["Signed vendor form"],
            bodies: &[
                "Hi Alex,\n\nCould you send me the signed vendor form by {due}? Finance can't release the first payment without it.\n\nThanks,\n{name}",
                "Thanks, got your note. I'll watch for it and pass it to finance as soon as it lands.\n\n{name}",
                "Received, thank you. Finance have it now.\n\n{name}",
            ],
        },
        Variant {
            id: "headcount",
            subjects: &["Updated headcount numbers"],
            bodies: &[
                "Hi Alex,\n\nPlease send me the updated headcount numbers before {due}. They go into the planning pack and that deadline doesn't move.\n\n{name}",
                "Perfect, I'll slot them in when they arrive.\n\n{name}",
                "Got them, thanks. The pack is complete.\n\n{name}",
            ],
        },
        Variant {
            id: "slides",
            subjects: &["Slides for the review"],
            bodies: &[
                "Hi Alex, can you share your slides for the review by {due}? I'm assembling the deck that afternoon and won't be able to wait for stragglers.\n\n{name}",
                "No problem, I'll keep a gap in the deck for yours.\n\n{name}",
                "They're in, and they look great. Thank you.\n\n{name}",
            ],
        },
        Variant {
            id: "expenses",
            subjects: &["Expense report"],
            bodies: &[
                "Alex, your expense report needs to be submitted no later than {due} or it rolls into next month. Can you take care of it?\n\n{name}",
                "Thanks for the heads up on your side. I'll approve it as soon as it's in.\n\n{name}",
                "Approved. You'll see it with the next payment.\n\n{name}",
            ],
        },
        Variant {
            id: "reference-call",
            subjects: &["Customer reference call"],
            bodies: &[
                "Hi Alex,\n\nWould you confirm the customer reference call by {due}? They need an answer to hold the slot.\n\n{name}",
                "Great, I'll tell them to expect your confirmation.\n\n{name}",
                "Confirmed with them. Thanks for sorting that so quickly.\n\n{name}",
            ],
        },
        Variant {
            id: "training",
            subjects: &["Security training"],
            bodies: &[
                "Hi Alex, a reminder that the security training must be completed by {due}. Please finish it and send me the certificate.\n\n{name}",
                "Thanks, I'll tick you off once the certificate arrives.\n\n{name}",
                "Certificate received, you're all done.\n\n{name}",
            ],
        },
    ],
};

pub(super) static HIRING_THREAD: Family = Family {
    id: "hiring-thread",
    senders: Senders::People { copied: false },
    subjects: Subjects::Thread,
    observer_cc: true,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "H",
    style: hiring_look,
    attachments: Attachments::Always,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: hiring,
    waiting: true,
    spam_folder: false,
    archived_every: None,
    turns: &[
        Turn::them(
            "bulk-hiring-candidate",
            "A hiring colleague shares a candidate's scorecard and asks for your view",
            CONVERSATION,
        ),
        Turn::you("bulk-hiring-your-feedback", "Your feedback"),
        Turn::them(
            "bulk-hiring-next-steps",
            "The colleague says what happens next",
            CONVERSATION,
        ),
    ],
    sent_replies: &[
        "Hi {name},\n\nThanks for the scorecard. Strong on systems debugging, and I'd want to probe product collaboration in the next round. I'm a yes to move forward.\n\nAlex",
        "Hi {name},\n\nRead it through. I liked the answer on incident ownership; less sure about the design exercise. Happy to take the next conversation.\n\nAlex",
        "{name}, my view is a hire with one reservation about scope of past work. Details on the form.\n\nAlex",
    ],
    variants: &[
        Variant {
            id: "staff-engineer",
            subjects: &["Interview panel for Staff Engineer candidate"],
            bodies: &[
                "Hi Alex,\n\nThe panel finished yesterday. Scorecard attached. The candidate was pragmatic on systems debugging; the open concern is limited product collaboration. What's your read?\n\n{name}",
                "Thanks for that. We'll go ahead and schedule the final round, and {other} will be in touch with the candidate.\n\n{name}",
            ],
        },
        Variant {
            id: "backend",
            subjects: &["Backend engineer shortlist"],
            bodies: &[
                "Hi Alex,\n\nShortlist attached with scores from the phone screens. Two stand out for me. I'd like your view before I put them forward for the on-site.\n\n{name}",
                "Understood, I'll put both forward. {other} will arrange the on-site days.\n\n{name}",
            ],
        },
        Variant {
            id: "designer",
            subjects: &["Feedback on the product designer loop"],
            bodies: &[
                "Hi Alex,\n\nAttached is the debrief from the product designer loop. Feedback was mostly positive, with some doubt about working with engineers early. Does that match what you saw?\n\n{name}",
                "Good to know. I'll take that into the decision meeting tomorrow.\n\n{name}",
            ],
        },
        Variant {
            id: "recruiter-update",
            subjects: &["Candidate update: data engineer"],
            bodies: &[
                "Alex,\n\nUpdate from the recruiter: the data engineer candidate has another offer on the table and wants an answer soon. Scorecard attached; can you tell me where you stand?\n\n{name}",
                "Thanks. We'll move quickly and make an offer this week if the others agree.\n\n{name}",
            ],
        },
    ],
};

pub(super) static COPIED_THREAD: Family = Family {
    id: "copied-thread",
    senders: Senders::People { copied: true },
    subjects: Subjects::Pool,
    observer_cc: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "C",
    style: plain,
    attachments: Attachments::None,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: work,
    waiting: false,
    spam_folder: false,
    archived_every: Some(11),
    turns: &COPIED_TURNS,
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "scheduling",
            subjects: &[],
            bodies: &[
                "Hi {other},\n\nCopying Alex for visibility. Are we still good to meet on {topic} this week? I can do Wednesday or Thursday afternoon.\n\n{name}",
                "Thursday works for me. I'll book a room.\n\n{name}",
                "Great, see you then. I'll bring the latest numbers.\n\n{name}",
                "Room's booked, 3pm. I've added the agenda to the invite.\n\n{name}",
                "Perfect. Thanks both.\n\n{name}",
            ],
        },
        Variant {
            id: "handover",
            subjects: &[],
            bodies: &[
                "Hi {other},\n\nHanding {topic} over to you as I'm out next week. Alex is copied so they know who to ask. Everything is in the shared folder.\n\n{name}",
                "Thanks for the handover. I've read the folder and it's clear. I'll take it from here.\n\n{name}",
                "Appreciate it. If anything comes up, my phone is on.\n\n{name}",
                "Will do. Enjoy the time off.\n\n{name}",
                "Thanks!\n\n{name}",
            ],
        },
        Variant {
            id: "review",
            subjects: &[],
            bodies: &[
                "Hi {other},\n\nI've reviewed the draft on {topic} and left comments. Alex copied as they asked to be kept in the loop.\n\n{name}",
                "Thanks, I've addressed most of them. The one on the data source I'd like to discuss.\n\n{name}",
                "Happy to talk. Do you have ten minutes after lunch?\n\n{name}",
                "Yes, after lunch is fine.\n\n{name}",
                "Resolved, I've updated the draft.\n\n{name}",
            ],
        },
        Variant {
            id: "vendor-quote",
            subjects: &[],
            bodies: &[
                "Hi {other},\n\nThe vendor sent their quote for {topic}. It's higher than we planned. I've copied Alex since it touches their budget.\n\n{name}",
                "I'm not surprised. Let's ask them to break out the support line and see what moves.\n\n{name}",
                "Agreed. I'll write back this afternoon.\n\n{name}",
                "They've come back with a lower number. Better, but still above target.\n\n{name}",
                "Okay, let's accept that and move on.\n\n{name}",
            ],
        },
    ],
};

const COPIED_WHY: &str = "You are only copied, so the thread is not addressed to you. The copied rule puts it in Updates and Now asks 'Not sure' (D119 U1); the not-sure mark is not one of the four questions. Same call as messages-copied-only.";

const COPIED_TURNS: [Turn; 5] = [
    Turn::them(
        "bulk-copied-opening",
        "A colleague writes to another colleague and copies you for visibility",
        COPIED,
    )
    .reviewed(&["home"], COPIED_WHY),
    Turn::them(
        "bulk-copied-reply",
        "The other colleague answers; you are still only copied",
        COPIED,
    )
    .reviewed(&["home"], COPIED_WHY),
    Turn::them(
        "bulk-copied-follow-up",
        "The first colleague replies again; you are still only copied",
        COPIED,
    )
    .reviewed(&["home"], COPIED_WHY),
    Turn::them(
        "bulk-copied-confirmation",
        "A colleague confirms the plan; you are still only copied",
        COPIED,
    )
    .reviewed(&["home"], COPIED_WHY),
    Turn::them(
        "bulk-copied-thanks",
        "A colleague thanks the other; you are still only copied",
        COPIED,
    )
    .reviewed(&["home"], COPIED_WHY),
];
