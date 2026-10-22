//! Pairs 1 to 10: a yes or a fact typed into the draft against the same
//! thread with the fact left to the instruction or left open, and selections.

use super::{
    expect, target, Answer, Case, DraftState, Message, Recording, JONAS, PRIYA, SAM, VOICE_ALEX,
    VOICE_DE,
};

const FRIDAY_THREAD: &[Message] = &[target(
    SAM,
    "Mon 19 Oct 2026, 09:14",
    "Can we meet Friday at 2pm to go through the contract?",
)];
const FRIDAY: &[Answer] = &[Answer {
    question: "when",
    any_of: &["friday"],
}];

const PRICE_THREAD: &[Message] = &[target(
    PRIYA,
    "Tue 20 Oct 2026, 11:02",
    "What's the price for the 10-seat plan per year?",
)];

const DATE_THREAD: &[Message] = &[target(
    SAM,
    "Wed 21 Oct 2026, 08:30",
    "When can you send the signed contract back?",
)];

const DECK_THREAD: &[Message] = &[target(
    PRIYA,
    "Thu 15 Oct 2026, 16:40",
    "Where are we on the board deck? Do you need anything from me?",
)];
const DECK: &[Answer] = &[Answer {
    question: "where the deck is",
    any_of: &["legal", "thursday"],
}];

const INTRO_THREAD: &[Message] = &[target(
    SAM,
    "Fri 16 Oct 2026, 10:05",
    "Could you introduce me to Priya? She runs the migration.",
)];

const GERMAN_THREAD: &[Message] = &[target(
    JONAS,
    "Mon 19 Oct 2026, 13:20",
    "Können wir uns Freitag um 14 Uhr treffen?",
)];
const GERMAN_WHEN: &[Answer] = &[Answer {
    question: "wann",
    any_of: &["freitag"],
}];

const DECLINE_THREAD: &[Message] = &[target(
    SAM,
    "Thu 15 Oct 2026, 17:12",
    "Can you join the offsite on Thursday 30 October? We'd love you there.",
)];
const DECLINE: &[Answer] = &[Answer {
    question: "can you join",
    any_of: &["can't", "won't", "cannot", "unable", "not able", "sorry"],
}];

const SLIDES_THREAD: &[Message] = &[target(
    SAM,
    "Tue 20 Oct 2026, 09:00",
    "Can you send the slides before Thursday's call?",
)];
const SLIDES: &[Answer] = &[Answer {
    question: "slides by when",
    any_of: &["wednesday", "tomorrow"],
}];

const ROADMAP_THREAD: &[Message] = &[target(
    PRIYA,
    "Mon 19 Oct 2026, 15:45",
    "Here's the draft roadmap. Could you give it a read before Monday?",
)];
const MONDAY: &[Answer] = &[Answer {
    question: "read it before Monday",
    any_of: &["monday"],
}];

const REVIEW_THREAD: &[Message] = &[target(
    SAM,
    "Wed 21 Oct 2026, 12:10",
    "Are we still on for the 3pm review tomorrow?",
)];
const STILL_ON: &[Answer] = &[Answer {
    question: "still on for it",
    any_of: &["yes", "still on"],
}];

pub(super) const CASES: &[Case] = &[
    Case {
        id: "friday-typed",
        description: "A yes to Friday, with two typed sentences to keep",
        twin_of: None,
        differs_by: "",
        thread: FRIDAY_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Friday at 2pm works for me. I'll bring the deck."),
        instruction: "",
        expect: expect(FRIDAY, &[], (8, 60)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Sam,\n\nFriday at 2pm works for me. I'll bring the deck.\n\nAlex",
            },
            Recording {
                name: "bad: dropped typed sentence",
                text: "Hi Sam,\n\nFriday at 2pm works.\n\nAlex",
            },
        ],
    },
    Case {
        id: "friday-empty",
        description: "A yes to Friday from the instruction alone",
        twin_of: Some("friday-typed"),
        differs_by: "nothing typed; the instruction carries the yes and the deck",
        thread: FRIDAY_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "say yes to Friday 2pm and that I'll bring the deck",
        expect: expect(FRIDAY, &[], (8, 60)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Sam,\n\nFriday at 2pm works. I'll bring the deck.\n\nAlex",
        }],
    },
    Case {
        id: "price-typed",
        description: "A price the user typed, which the reply has to carry",
        twin_of: None,
        differs_by: "",
        thread: PRICE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("It's £480 a year, billed annually."),
        instruction: "",
        expect: expect(
            &[Answer {
                question: "price",
                any_of: &["480"],
            }],
            &[],
            (8, 60),
        ),
        recorded: &[Recording {
            name: "good",
            text: "Hi Priya,\n\nIt's £480 a year, billed annually.\n\nAlex",
        }],
    },
    Case {
        id: "price-open",
        description: "A price nobody has given: leave a gap, invent nothing",
        twin_of: Some("price-typed"),
        differs_by: "nothing typed; the price isn't known",
        thread: PRICE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "",
        expect: expect(&[], &["price"], (8, 60)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Priya,\n\nThanks for asking. I'll confirm [[?: price per year for the 10-seat plan]] and come back to you.\n\nAlex",
            },
            Recording {
                name: "bad: invented price",
                text: "Hi Priya,\n\nThe 10-seat plan is £450 a year, billed annually.\n\nAlex",
            },
        ],
    },
    Case {
        id: "date-typed",
        description: "A date the user typed in answer to a question",
        twin_of: None,
        differs_by: "",
        thread: DATE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Wednesday, after lunch."),
        instruction: "",
        expect: expect(
            &[Answer {
                question: "when",
                any_of: &["wednesday"],
            }],
            &[],
            (6, 40),
        ),
        recorded: &[Recording {
            name: "good",
            text: "Hi Sam,\n\nWednesday, after lunch.\n\nAlex",
        }],
    },
    Case {
        id: "date-open",
        description: "A question the user has not answered: leave a gap",
        twin_of: Some("date-typed"),
        differs_by: "nothing typed; the user hasn't decided the day",
        thread: DATE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "",
        expect: expect(&[], &["when"], (6, 40)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Sam,\n\nI'll send it back once I know [[?: when I can sign it]].\n\nAlex",
        }],
    },
    Case {
        id: "deck-typed",
        description: "A status the user typed, with the facts in it",
        twin_of: None,
        differs_by: "",
        thread: DECK_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("It's with legal and back Thursday."),
        instruction: "",
        expect: expect(DECK, &[], (6, 50)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Priya,\n\nIt's with legal and back Thursday.\n\nAlex",
        }],
    },
    Case {
        id: "deck-empty",
        description: "The same status, given in the instruction instead",
        twin_of: Some("deck-typed"),
        differs_by: "nothing typed; the facts come in the instruction",
        thread: DECK_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "say the deck is with legal and back Thursday, and that I need nothing from her",
        expect: expect(DECK, &[], (6, 50)),
        recorded: &[],
    },
    Case {
        id: "intro-typed",
        description: "A short agreement the user typed",
        twin_of: None,
        differs_by: "",
        thread: INTRO_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Happy to intro you, sending the note now."),
        instruction: "",
        expect: expect(&[], &[], (6, 50)),
        recorded: &[
            Recording {
                name: "good",
                text: "Hi Sam,\n\nHappy to intro you, sending the note now.\n\nAlex",
            },
            Recording {
                name: "bad: subject line",
                text: "Subject: Re: Intro to Priya\n\nHi Sam,\n\nHappy to intro you, sending the note now.\n\nAlex",
            },
        ],
    },
    Case {
        id: "intro-empty",
        description: "The same agreement, from the instruction",
        twin_of: Some("intro-typed"),
        differs_by: "nothing typed; the instruction agrees to the intro",
        thread: INTRO_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "agree to the intro and say I'll copy her in",
        expect: expect(&[], &[], (6, 50)),
        recorded: &[],
    },
    Case {
        id: "german-typed",
        description: "German typed text stays German",
        twin_of: None,
        differs_by: "",
        thread: GERMAN_THREAD,
        voice: VOICE_DE,
        draft: DraftState::Typed("Freitag um 14 Uhr passt mir."),
        instruction: "",
        expect: expect(GERMAN_WHEN, &[], (4, 40)),
        recorded: &[Recording {
            name: "good",
            text: "Hallo Jonas,\n\nFreitag um 14 Uhr passt mir.\n\nAlex",
        }],
    },
    Case {
        id: "german-empty",
        description: "A German reply from the instruction, in the thread's language",
        twin_of: Some("german-typed"),
        differs_by: "nothing typed; the instruction gives the day",
        thread: GERMAN_THREAD,
        voice: VOICE_DE,
        draft: DraftState::Empty,
        instruction: "ja, Freitag 14 Uhr passt",
        expect: expect(GERMAN_WHEN, &[], (4, 40)),
        recorded: &[],
    },
    Case {
        id: "decline-typed",
        description: "A polite no the user typed",
        twin_of: None,
        differs_by: "",
        thread: DECLINE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Sorry, I can't make it this time."),
        instruction: "",
        expect: expect(DECLINE, &[], (6, 50)),
        recorded: &[],
    },
    Case {
        id: "decline-empty",
        description: "The same no, from the instruction",
        twin_of: Some("decline-typed"),
        differs_by: "nothing typed; the instruction says no",
        thread: DECLINE_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "decline politely, I can't make it this time",
        expect: expect(DECLINE, &[], (6, 50)),
        recorded: &[],
    },
    Case {
        id: "slides-typed",
        description: "A date for the slides the user typed",
        twin_of: None,
        differs_by: "",
        thread: SLIDES_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Typed("Yes, sending them by Wednesday night."),
        instruction: "",
        expect: expect(SLIDES, &[], (5, 40)),
        recorded: &[],
    },
    Case {
        id: "slides-empty",
        description: "The same date, from the instruction",
        twin_of: Some("slides-typed"),
        differs_by: "nothing typed; the instruction gives the date",
        thread: SLIDES_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "yes, I'll send them Wednesday night",
        expect: expect(SLIDES, &[], (5, 40)),
        recorded: &[],
    },
    Case {
        id: "warmer-selection",
        description: "A selected sentence to make warmer, with the rest kept",
        twin_of: None,
        differs_by: "",
        thread: ROADMAP_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Selection {
            typed: "Thanks for sending this over. Happy to take a look Monday.",
            start: 30,
            end: 58,
        },
        instruction: "make the selected sentence warmer",
        expect: expect(MONDAY, &[], (8, 60)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Priya,\n\nThanks for sending this over. I'd be glad to read it properly on Monday.\n\nAlex",
        }],
    },
    Case {
        id: "warmer-empty",
        description: "The same reply, asked for warmer without a selection",
        twin_of: Some("warmer-selection"),
        differs_by: "nothing typed or selected; the instruction says what to keep",
        thread: ROADMAP_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "thank her for sending it, warm, I'll read it properly Monday",
        expect: expect(MONDAY, &[], (8, 60)),
        recorded: &[],
    },
    Case {
        id: "shorter-selection",
        description: "A selected clause to cut, with the yes kept",
        twin_of: None,
        differs_by: "",
        thread: REVIEW_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Selection {
            typed: "Yes, I'm still on for it. I've moved two other things so I can make it properly.",
            start: 26,
            end: 80,
        },
        instruction: "shorter",
        expect: expect(STILL_ON, &[], (4, 30)),
        recorded: &[Recording {
            name: "good",
            text: "Hi Sam,\n\nYes, I'm still on for it.\n\nAlex",
        }],
    },
    Case {
        id: "shorter-empty",
        description: "The same yes, from the instruction",
        twin_of: Some("shorter-selection"),
        differs_by: "nothing typed or selected; the instruction gives the yes",
        thread: REVIEW_THREAD,
        voice: VOICE_ALEX,
        draft: DraftState::Empty,
        instruction: "shorter: yes, still on for the 3pm tomorrow",
        expect: expect(STILL_ON, &[], (4, 30)),
        recorded: &[],
    },
];
