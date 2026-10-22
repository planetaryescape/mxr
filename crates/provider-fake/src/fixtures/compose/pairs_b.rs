//! Pairs 11 to 20: prices and facts the thread does or does not carry, the
//! voice's sign-off and dashes, a reply to a longer thread, short acks, seats,
//! apologies, two questions in one message, and a question in typed text.

use super::{
    expect, mine, target, them, Answer, Case, DraftState, Message, Recording, PRIYA, SAM,
    VOICE_ALEX, VOICE_DASHES, VOICE_INITIAL,
};

const INVOICE_WITH_PRICE: &[Message] = &[target(
    PRIYA,
    "Fri 16 Oct 2026, 09:00",
    "The workshop invoice is £1,200, due 31 October. Have you paid it yet?",
)];
const INVOICE_NO_PRICE: &[Message] = &[target(
    PRIYA,
    "Fri 16 Oct 2026, 09:00",
    "Have you paid the workshop invoice yet?",
)];
const PAID: &[Answer] = &[Answer {
    question: "paid",
    any_of: &["paid"],
}];

const LUNCH_THREAD: &[Message] = &[target(SAM, "Thu 22 Oct 2026, 08:10", "Lunch on Friday?")];
const FRIDAY_LUNCH: &[Answer] = &[Answer {
    question: "lunch on Friday",
    any_of: &["friday"],
}];

const REVIEW_THURSDAY_THREAD: &[Message] = &[target(
    SAM,
    "Wed 21 Oct 2026, 07:50",
    "Can you do Thursday for the review?",
)];
const THURSDAY: &[Answer] = &[Answer {
    question: "Thursday",
    any_of: &["thursday"],
}];

const CLAUSE_THREAD: &[Message] = &[
    them(
        SAM,
        "Mon 12 Oct 2026, 09:00",
        "We're finalising the Q3 partner terms. Legal flagged clause 4 on exclusivity.",
    ),
    mine(
        "Mon 12 Oct 2026, 16:00",
        "Thanks. I'll look at clause 4 with our counsel this week.",
    ),
    target(
        SAM,
        "Thu 15 Oct 2026, 16:20",
        "Any update on clause 4? The board wants to sign by 30 October. Could we talk Monday?",
    ),
];
const MONDAY_10: &[Answer] = &[Answer {
    question: "when to talk",
    any_of: &["monday"],
}];

const CATCH_UP_THREAD: &[Message] = &[target(
    SAM,
    "Wed 21 Oct 2026, 09:30",
    "Do you have time for a catch-up this month?",
)];
const THURSDAY_MORNING: &[Answer] = &[Answer {
    question: "a time",
    any_of: &["thursday"],
}];

const ACK_THREAD: &[Message] = &[target(
    SAM,
    "Wed 21 Oct 2026, 08:00",
    "Sent you the updated pricing sheet, see attached.",
)];

const SEATS_THREAD: &[Message] = &[target(
    PRIYA,
    "Tue 20 Oct 2026, 14:15",
    "How many seats do you need for the workshop?",
)];
const SEATS: &[Answer] = &[Answer {
    question: "how many seats",
    any_of: &["12"],
}];

const REPORT_THREAD: &[Message] = &[target(
    SAM,
    "Mon 19 Oct 2026, 18:02",
    "The report was due Friday and we still don't have it. What's happening?",
)];
const WHY_LATE: &[Answer] = &[Answer {
    question: "why it's late",
    any_of: &["travel", "delay", "sorry"],
}];

const CALL_THREAD: &[Message] = &[target(
    SAM,
    "Wed 21 Oct 2026, 07:45",
    "Can we move to 3pm? Will Priya join the call?",
)];
const TWO_QUESTIONS: &[Answer] = &[
    Answer {
        question: "is 3pm fine",
        any_of: &["3pm"],
    },
    Answer {
        question: "will Priya join",
        any_of: &["no", "not", "can't", "won't", "out"],
    },
];

const QUOTE_THREAD: &[Message] = &[target(
    PRIYA,
    "Thu 22 Oct 2026, 10:00",
    "Could you send a quote for the workshop? Let me know the rate per person.",
)];
const RATE: &[Answer] = &[Answer {
    question: "rate per person",
    any_of: &["85"],
}];

pub(super) const CASES: &[Case] = &[
    Case {
        id: "invoice-typed-price",
        description: "A payment confirmed, with the amount in the thread",
        twin_of: None,
        differs_by: "",
        thread: INVOICE_WITH_PRICE,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Paid today, thanks."),
        instruction: "",
        expect: expect(PAID, &[], (3, 30)),
        recorded: &[],
    },
    Case {
        id: "invoice-typed-no-price",
        description: "The same confirmation, with no amount in the thread",
        twin_of: Some("invoice-typed-price"),
        differs_by: "the invoice amount is not in the thread",
        thread: INVOICE_NO_PRICE,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Paid today, thanks."),
        instruction: "",
        expect: expect(PAID, &[], (3, 30)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Priya,\n\nPaid today, thanks.\n\nAlex",
            },
            Recording {
                name: "bad: invented price",
                text: "Hi Priya,\n\nPaid today, thanks. The £1,200 is on its way.\n\nAlex",
            },
        ],
    },
    Case {
        id: "lunch-initial-voice",
        description: "A yes to lunch in a voice that signs with an initial",
        twin_of: None,
        differs_by: "",
        thread: LUNCH_THREAD,
        voice: VOICE_INITIAL,
        draft: DraftState::Typed("Friday lunch works for me."),
        instruction: "",
        expect: expect(FRIDAY_LUNCH, &[], (4, 40)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Sam,\n\nFriday lunch works for me.\n\nA",
        }],
    },
    Case {
        id: "lunch-first-name-voice",
        description: "The same yes, in a voice that signs with a first name",
        twin_of: Some("lunch-initial-voice"),
        differs_by: "the voice signs with a first name, not an initial",
        thread: LUNCH_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Friday lunch works for me."),
        instruction: "",
        expect: expect(FRIDAY_LUNCH, &[], (4, 40)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Sam,\n\nFriday lunch works for me.\n\nAlex",
        }],
    },
    Case {
        id: "review-dashes-voice",
        description: "A yes in a voice that uses em dashes",
        twin_of: None,
        differs_by: "",
        thread: REVIEW_THURSDAY_THREAD,
        voice: VOICE_DASHES,
        draft: DraftState::Typed("Thursday works for me."),
        instruction: "",
        expect: expect(THURSDAY, &[], (3, 40)),
        recorded: &[],
    },
    Case {
        id: "review-plain-voice",
        description: "The same yes in a voice that uses no dashes",
        twin_of: Some("review-dashes-voice"),
        differs_by: "the voice uses no em dashes",
        thread: REVIEW_THURSDAY_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Thursday works for me."),
        instruction: "",
        expect: expect(THURSDAY, &[], (3, 40)),
        recorded: &[],
    },
    Case {
        id: "clause-long-thread",
        description: "A reply to a three-message thread, with the user's typed answer",
        twin_of: None,
        differs_by: "",
        thread: CLAUSE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Monday at 10am works."),
        instruction: "I'll have counsel's note by then",
        expect: expect(MONDAY_10, &[], (6, 60)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Sam,\n\nMonday at 10am works. I'll have counsel's note on clause 4 by then.\n\nAlex",
            },
            Recording {
                name: "bad: recap of thread",
                text: "Hi Sam,\n\nYou asked for an update on clause 4. The board wants to sign by 30 October, so Monday at 10am works.\n\nAlex",
            },
        ],
    },
    Case {
        id: "clause-long-thread-empty",
        description: "The same reply from the instruction, with no typed text",
        twin_of: Some("clause-long-thread"),
        differs_by: "nothing typed; the instruction gives the time",
        thread: CLAUSE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "Monday at 10am works, I'll have counsel's note by then",
        expect: expect(MONDAY_10, &[], (6, 60)),
        recorded: &[],
    },
    Case {
        id: "ack-typed",
        description: "A short acknowledgement the user typed",
        twin_of: None,
        differs_by: "",
        thread: ACK_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Thanks, got it."),
        instruction: "",
        expect: expect(&[], &[], (2, 30)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Sam,\n\nThanks, got it.\n\nAlex",
            },
            Recording {
                name: "bad: duplicate signature",
                text: "Hi Sam,\n\nThanks, got it.\n\nAlex\n\nThanks,\nAlex",
            },
        ],
    },
    Case {
        id: "ack-empty",
        description: "The same acknowledgement, from the instruction",
        twin_of: Some("ack-typed"),
        differs_by: "nothing typed; the instruction says it",
        thread: ACK_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "thanks, got it",
        expect: expect(&[], &[], (2, 30)),
        recorded: &[],
    },
    Case {
        id: "seats-typed",
        description: "A number of seats the user typed",
        twin_of: None,
        differs_by: "",
        thread: SEATS_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("We can do 12 seats."),
        instruction: "",
        expect: expect(SEATS, &[], (4, 40)),
        recorded: &[],
    },
    Case {
        id: "seats-open",
        description: "A number of seats the user has not decided: leave a gap",
        twin_of: Some("seats-typed"),
        differs_by: "nothing typed; the number isn't known",
        thread: SEATS_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "",
        expect: expect(&[], &["seats"], (4, 40)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Priya,\n\nThanks. I'll confirm [[?: how many seats we can offer]] with the team.\n\nAlex",
        }],
    },
    Case {
        id: "late-typed",
        description: "An apology the user typed, with the reason",
        twin_of: None,
        differs_by: "",
        thread: REPORT_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Sorry for the delay, I was travelling."),
        instruction: "",
        expect: expect(WHY_LATE, &[], (6, 50)),
        recorded: &[],
    },
    Case {
        id: "late-empty",
        description: "The same apology, from the instruction",
        twin_of: Some("late-typed"),
        differs_by: "nothing typed; the instruction gives the reason",
        thread: REPORT_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "apologise, I was travelling, report by Wednesday",
        expect: expect(WHY_LATE, &[], (6, 50)),
        recorded: &[],
    },
    Case {
        id: "call-typed",
        description: "Answers to two questions, typed",
        twin_of: None,
        differs_by: "",
        thread: CALL_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("3pm is fine. Priya is out that day, so no."),
        instruction: "",
        expect: expect(TWO_QUESTIONS, &[], (5, 50)),
        recorded: &[],
    },
    Case {
        id: "call-empty",
        description: "The same two answers, from the instruction",
        twin_of: Some("call-typed"),
        differs_by: "nothing typed; the instruction gives both answers",
        thread: CALL_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "3pm works; Priya can't make it",
        expect: expect(TWO_QUESTIONS, &[], (5, 50)),
        recorded: &[],
    },
    Case {
        id: "rate-from-instruction",
        description: "A rate given in the instruction, which the reply must use",
        twin_of: None,
        differs_by: "",
        thread: QUOTE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "the rate is 85 per person, invoice on the day",
        expect: expect(RATE, &[], (6, 60)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Priya,\n\nThe rate is £85 per person, with the invoice on the day.\n\nAlex",
            },
            Recording {
                name: "bad: invented price",
                text: "Hi Priya,\n\nThe rate is £95 per person, with the invoice on the day.\n\nAlex",
            },
        ],
    },
    Case {
        id: "rate-open",
        description: "A rate nobody has given: leave a gap",
        twin_of: Some("rate-from-instruction"),
        differs_by: "the instruction gives no rate",
        thread: QUOTE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "",
        expect: expect(&[], &["rate"], (6, 60)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Priya,\n\nI'll send the workshop quote with [[?: rate per person]] shortly.\n\nAlex",
        }],
    },
    Case {
        id: "catch-up-typed-question",
        description: "A typed reply that is itself a question, then an offer",
        twin_of: None,
        differs_by: "",
        thread: CATCH_UP_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Are you free Thursday morning? I can do 10am."),
        instruction: "",
        expect: expect(THURSDAY_MORNING, &[], (5, 50)),
        recorded: &[],
    },
    Case {
        id: "catch-up-empty",
        description: "The same suggestion, from the instruction",
        twin_of: Some("catch-up-typed-question"),
        differs_by: "nothing typed; the instruction suggests the time",
        thread: CATCH_UP_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "suggest Thursday morning at 10",
        expect: expect(THURSDAY_MORNING, &[], (5, 50)),
        recorded: &[],
    },
];
