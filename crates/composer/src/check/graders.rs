//! The six graders. Each reads a case's requirements and an output and
//! returns one grade that names what was required and what happened.

use super::text::{contains_phrase, copied_run, gap_texts, invented_numbers, sentences, words};
use super::{Check, Grade, Output, Requirements};
use std::collections::HashSet;

/// A typed sentence counts as changed, not missing, when this share of its
/// words still appears in the draft.
const CHANGED_SHARE: f64 = 0.8;

pub(super) fn kept(req: &Requirements<'_>, output: &Output<'_>) -> Grade {
    let typed = sentences(req.typed);
    let requirement = if typed.is_empty() {
        "nothing typed to keep".to_string()
    } else {
        format!(
            "keep each typed sentence: {}",
            typed
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    // Whole sentences: a typed sentence embedded in a changed one is not kept.
    let draft_sentences: HashSet<String> = sentences(output.text).into_iter().collect();
    let draft_words: HashSet<String> = words(output.text).into_iter().collect();
    let mut problems = Vec::new();
    for sentence in &typed {
        if draft_sentences.contains(sentence) {
            continue;
        }
        let sentence_words = words(sentence);
        let found = sentence_words
            .iter()
            .filter(|word| draft_words.contains(*word))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let share = found as f64 / sentence_words.len().max(1) as f64;
        problems.push(if share >= CHANGED_SHARE {
            format!("changed: \"{sentence}\"")
        } else {
            format!("missing: \"{sentence}\"")
        });
    }
    let detail = if typed.is_empty() {
        "no typed text".to_string()
    } else if problems.is_empty() {
        format!("all {} kept", typed.len())
    } else {
        problems.join("; ")
    };
    Grade {
        check: Check::Kept,
        pass: problems.is_empty(),
        requirement,
        detail,
    }
}

pub(super) fn answers(req: &Requirements<'_>, output: &Output<'_>) -> Grade {
    let draft_words = words(output.text);
    let requirement = if req.answers.is_empty() {
        "no question to answer".to_string()
    } else {
        req.answers
            .iter()
            .map(|answer| {
                format!(
                    "answer \"{}\" (any of: {})",
                    answer.question,
                    answer.any_of.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let unanswered: Vec<&str> = req
        .answers
        .iter()
        .filter(|answer| {
            !answer
                .any_of
                .iter()
                .any(|phrase| contains_phrase(&draft_words, phrase))
        })
        .map(|answer| answer.question)
        .collect();
    let detail = if req.answers.is_empty() {
        "nothing to check".to_string()
    } else if unanswered.is_empty() {
        format!("all {} answered", req.answers.len())
    } else {
        format!("not addressed: {}", unanswered.join(", "))
    };
    Grade {
        check: Check::Answers,
        pass: unanswered.is_empty(),
        requirement,
        detail,
    }
}

pub(super) fn gaps(req: &Requirements<'_>, output: &Output<'_>) -> Grade {
    let gaps = gap_texts(output.text);
    let missing: Vec<&str> = req
        .gaps
        .iter()
        .copied()
        .filter(|fact| !gaps.iter().any(|gap| contains_phrase(&words(gap), fact)))
        .collect();
    let requirement = if req.gaps.is_empty() {
        "no missing fact to leave open".to_string()
    } else {
        format!("leave a [[?: …]] gap for: {}", req.gaps.join(", "))
    };
    let detail = if req.gaps.is_empty() {
        format!("{} gap(s) written", gaps.len())
    } else if missing.is_empty() {
        format!("gap written for all {}", req.gaps.len())
    } else {
        format!("no gap for: {}", missing.join(", "))
    };
    Grade {
        check: Check::Gaps,
        pass: missing.is_empty(),
        requirement,
        detail,
    }
}

pub(super) fn invented(req: &Requirements<'_>, output: &Output<'_>) -> Grade {
    let known = [
        req.conversation,
        req.typed,
        req.instruction,
        &req.allowed_numbers.join(" "),
    ]
    .join("\n");
    let invented = invented_numbers(output.text, &known);
    let detail = if invented.is_empty() {
        "no number outside the conversation, typed text or instruction".to_string()
    } else {
        format!("not in any input: {}", invented.join(", "))
    };
    Grade {
        check: Check::Invented,
        pass: invented.is_empty(),
        requirement: "no number, date or amount that no input carries".to_string(),
        detail,
    }
}

pub(super) fn length(req: &Requirements<'_>, output: &Output<'_>) -> Grade {
    let count = u32::try_from(output.text.split_whitespace().count()).unwrap_or(u32::MAX);
    let band = req.words;
    Grade {
        check: Check::Length,
        pass: (band.min..=band.max).contains(&count),
        requirement: format!("{} to {} words", band.min, band.max),
        detail: format!("{count} words"),
    }
}

/// A line of commentary about generating the reply: "Sure, here's a draft of
/// your reply.", "Here is the draft reply.", "Here is my reply:". It opens
/// with a lead word, then says the reply is here, below, or drafted. The
/// lead must end at a word boundary, and the commentary must be explicit: an
/// ordinary email sentence such as "Sure, here is the deck you asked for"
/// or "Sure, thanks for the draft roadmap" is part of the email.
fn is_preamble(line: &str) -> bool {
    // Models and keyboards differ on the apostrophe, so "Here’s" counts.
    let lower = line.trim().to_lowercase().replace('\u{2019}', "'");
    let leads = [
        "sure",
        "here",
        "certainly",
        "of course",
        "okay",
        "ok",
        "absolutely",
        "below",
        "alright",
    ];
    let opens_with_lead = leads.iter().any(|lead| {
        lower.strip_prefix(lead).is_some_and(|rest| {
            rest.chars().next().is_none_or(|character| {
                character.is_whitespace() || character.is_ascii_punctuation()
            })
        })
    });
    opens_with_lead && says_the_reply_is_here(&lower)
}

/// Commentary that names the reply or its draft: "here's a draft", "here is
/// my reply", "below is the reply", or the fixed phrases below.
fn says_the_reply_is_here(lower: &str) -> bool {
    const INTRODUCERS: [&str; 3] = ["here's ", "here is ", "below is "];
    const OBJECTS: [&str; 11] = [
        "a draft",
        "the draft",
        "your draft",
        "my draft",
        "a reply",
        "the reply",
        "your reply",
        "my reply",
        "a version",
        "your version",
        "my version",
    ];
    const PHRASES: [&str; 5] = [
        "draft reply",
        "draft of your reply",
        "draft:",
        "reply:",
        "your version:",
    ];
    let introduces_the_reply = INTRODUCERS.iter().any(|introducer| {
        lower
            .find(introducer)
            .and_then(|at| lower.get(at + introducer.len()..))
            .is_some_and(|rest| OBJECTS.iter().any(|object| rest.starts_with(object)))
    });
    introduces_the_reply || PHRASES.iter().any(|phrase| lower.contains(phrase))
}

/// What clean-up had to remove, what a sign-off repeated, and whether a
/// run of the thread was copied back. Read from the raw output.
pub(super) fn leftovers(req: &Requirements<'_>, output: &Output<'_>) -> Grade {
    let text_lines: HashSet<&str> = output.text.lines().map(str::trim).collect();
    let mut problems: Vec<String> = Vec::new();

    let removed: Vec<&str> = output
        .raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !text_lines.contains(line))
        .collect();
    let subject = output
        .raw
        .lines()
        .map(str::trim)
        .find(|line| line.to_lowercase().starts_with("subject:"));
    if let Some(subject) = subject {
        problems.push(format!("subject line: \"{subject}\""));
    }
    // Read from the raw output, so a preamble that clean-up would also have
    // stripped still counts, and one it would keep is caught too.
    if let Some(line) = output
        .raw
        .lines()
        .map(str::trim)
        .find(|line| is_preamble(line))
    {
        problems.push(format!("preamble: \"{line}\""));
    }
    if !removed.is_empty() {
        problems.push(format!(
            "clean-up removed: {}",
            removed
                .iter()
                .map(|l| format!("\"{l}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let tail: Vec<&str> = output
        .raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(4)
        .collect();
    for (index, line) in tail.iter().enumerate() {
        let short = line.split_whitespace().count() <= 3;
        if short && tail[index + 1..].contains(line) {
            problems.push(format!("sign-off repeated: \"{line}\""));
            break;
        }
    }

    if let Some(run) = copied_run(output.raw, req.conversation) {
        problems.push(format!("copies the thread back: \"{run}\""));
    }

    let detail = if problems.is_empty() {
        "clean".to_string()
    } else {
        problems.join("; ")
    };
    Grade {
        check: Check::Leftovers,
        pass: problems.is_empty(),
        requirement: "no subject line, preamble, fence or placeholder, no repeated sign-off, and no run of 8 words copied from the thread".to_string(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{grade, Answer, Report, WordBand};
    use super::*;

    const THREAD: &str = "Can we meet Friday at 2pm? The budget is $4,000 and the \
        board wants the Q3 numbers before the call on Thursday.";

    fn req<'a>(typed: &'a str, answers: &'a [Answer<'a>], gaps: &'a [&'a str]) -> Requirements<'a> {
        Requirements {
            typed,
            instruction: "",
            conversation: THREAD,
            answers,
            gaps,
            allowed_numbers: &[],
            words: WordBand { min: 3, max: 60 },
        }
    }

    fn run(requirements: &Requirements<'_>, raw: &str) -> Report {
        let text = raw.trim();
        grade(requirements, &Output { raw, text })
    }

    #[test]
    fn a_typed_sentence_must_survive_and_a_changed_one_is_named() {
        let typed = "Friday at 2pm works for me. I'll bring the deck.";
        let requirements = req(typed, &[], &[]);
        let kept = run(
            &requirements,
            "Hi Alex,\n\nFriday at 2pm works for me. I\u{2019}ll bring the deck.\n\nAlex",
        );
        assert_eq!(kept.passed(Check::Kept), Some(true), "{kept:?}");
        let dropped = run(&requirements, "Hi Alex,\n\nFriday at 2pm works.\n\nAlex");
        assert_eq!(dropped.passed(Check::Kept), Some(false));
        let detail = &dropped.grades[0].detail;
        assert!(
            detail.contains("missing: \"friday at 2pm works for me\""),
            "{detail}"
        );
        assert!(
            detail.contains("missing: \"i'll bring the deck\""),
            "{detail}"
        );
        // Most of the words survive, so the sentence is changed, not gone.
        let changed = run(
            &requirements,
            "Hi Alex,\n\nFriday at two pm works for me.\n\nAlex",
        );
        let detail = &changed.grades[0].detail;
        assert!(
            detail.contains("changed: \"friday at 2pm works for me\""),
            "{detail}"
        );
    }

    #[test]
    fn nothing_typed_keeps_nothing_and_passes() {
        let report = run(&req("", &[], &[]), "Sounds good.");
        assert_eq!(report.passed(Check::Kept), Some(true));
        assert_eq!(report.grades[0].requirement, "nothing typed to keep");
    }

    #[test]
    fn an_answer_passes_when_any_phrase_is_there_and_fails_otherwise() {
        let answers = [Answer {
            question: "when",
            any_of: &["friday", "2pm"],
        }];
        let requirements = req("", &answers, &[]);
        assert_eq!(
            run(&requirements, "Hi, 2pm works.").passed(Check::Answers),
            Some(true)
        );
        let missed = run(&requirements, "Hi, sounds good.");
        assert_eq!(missed.passed(Check::Answers), Some(false));
        assert_eq!(missed.grades[1].detail, "not addressed: when");
    }

    #[test]
    fn a_gap_passes_only_when_a_gap_names_the_missing_fact() {
        let requirements = req("", &[], &["price"]);
        let with_gap = run(&requirements, "The rate is [[?: price per seat]].");
        assert_eq!(with_gap.passed(Check::Gaps), Some(true));
        // A gap for something else, or a guessed number, does not count.
        let guessed = run(&requirements, "The rate is [[?: date]] and £500.");
        assert_eq!(guessed.passed(Check::Gaps), Some(false));
        let no_gap = run(&requirements, "The rate is £500.");
        assert_eq!(no_gap.passed(Check::Gaps), Some(false));
    }

    #[test]
    fn a_number_no_input_carries_is_invented() {
        let requirements = req("", &[], &[]);
        assert_eq!(
            run(&requirements, "I can do $4,000 on Friday.").passed(Check::Invented),
            Some(true),
            "$4,000 and Friday are in the thread"
        );
        let invented = run(&requirements, "I can do $500 on Friday.");
        assert_eq!(invented.passed(Check::Invented), Some(false));
        assert_eq!(invented.grades[3].detail, "not in any input: 500");
    }

    #[test]
    fn a_number_from_the_typed_text_or_instruction_is_not_invented() {
        let mut requirements = req("We can do 12 seats.", &[], &[]);
        requirements.instruction = "say yes to the 3 o'clock slot";
        let report = run(
            &requirements,
            "We can do 12 seats, the 3 o'clock slot is fine.",
        );
        assert_eq!(report.passed(Check::Invented), Some(true));
    }

    #[test]
    fn length_is_inclusive_of_both_ends_and_fails_outside_them() {
        let requirements = req("", &[], &[]);
        assert_eq!(
            run(&requirements, "one two three").passed(Check::Length),
            Some(true)
        );
        assert_eq!(
            run(&requirements, "one two").passed(Check::Length),
            Some(false)
        );
        let long = vec!["word"; 61].join(" ");
        assert_eq!(run(&requirements, &long).passed(Check::Length), Some(false));
    }

    #[test]
    fn a_subject_line_is_a_leftover_even_though_clean_up_removes_it() {
        let requirements = req("", &[], &[]);
        let raw = "Subject: Re: Friday\n\nHi Alex,\n\nFriday works.\n\nAlex";
        let text = "Hi Alex,\n\nFriday works.\n\nAlex";
        let report = grade(&requirements, &Output { raw, text });
        assert_eq!(report.passed(Check::Leftovers), Some(false));
        assert!(report.grades[5]
            .detail
            .contains("subject line: \"Subject: Re: Friday\""));
    }

    #[test]
    fn a_fence_or_preamble_that_clean_up_removed_is_a_leftover() {
        let requirements = req("", &[], &[]);
        let raw = "Here's a draft:\n\nHi Alex,\n\nFriday works.\n\nAlex";
        let text = "Hi Alex,\n\nFriday works.\n\nAlex";
        let report = grade(&requirements, &Output { raw, text });
        assert_eq!(report.passed(Check::Leftovers), Some(false));
        assert!(report.grades[5].detail.contains("\"Here's a draft:\""));
    }

    #[test]
    fn a_repeated_sign_off_is_a_leftover_and_a_single_one_is_not() {
        let requirements = req("", &[], &[]);
        let twice = "Hi Alex,\n\nGot it.\n\nCheers,\nAlex\n\nCheers,\nAlex";
        let report = run(&requirements, twice);
        assert_eq!(report.passed(Check::Leftovers), Some(false));
        assert!(report.grades[5]
            .detail
            .contains("sign-off repeated: \"Alex\""));
        let once = "Hi Alex,\n\nGot it.\n\nCheers,\nAlex";
        assert_eq!(
            run(&requirements, once).passed(Check::Leftovers),
            Some(true)
        );
    }

    #[test]
    fn a_run_of_the_thread_copied_back_is_a_leftover_and_a_short_phrase_is_not() {
        let requirements = req("", &[], &[]);
        let recap =
            "You said the budget is $4,000 and the board wants the Q3 numbers. Friday works.";
        let report = run(&requirements, recap);
        assert_eq!(report.passed(Check::Leftovers), Some(false));
        assert!(report.grades[5].detail.contains("copies the thread back"));
        // Eight words, so a reply can reuse a short phrase from the thread.
        let short = "Friday at 2pm works for me, and I can bring the Q3 numbers.";
        assert_eq!(
            run(&requirements, short).passed(Check::Leftovers),
            Some(true)
        );
    }

    #[test]
    fn a_clean_draft_passes_every_graded_check() {
        let answers = [Answer {
            question: "when",
            any_of: &["friday"],
        }];
        let requirements = req("Friday at 2pm works for me.", &answers, &[]);
        let report = run(
            &requirements,
            "Hi Alex,\n\nFriday at 2pm works for me. Happy to do it.\n\nAlex",
        );
        assert!(report.all_pass(), "{:?}", report.grades);
        assert!(report.style.score <= 100);
    }

    #[test]
    fn a_grade_never_depends_on_the_case_order_of_its_lists() {
        // Each check reads the whole list, so reordering the answers or gaps
        // cannot change which one fails.
        let first = Answer {
            question: "when",
            any_of: &["friday"],
        };
        let second = Answer {
            question: "where",
            any_of: &["room 4"],
        };
        let forward = [first, second];
        let backward = [second, first];
        let text = "Hi, Friday is fine.\n\nAlex";
        let a = run(&req("", &forward, &[]), text);
        let b = run(&req("", &backward, &[]), text);
        assert_eq!(a.passed(Check::Answers), b.passed(Check::Answers));
        assert_eq!(a.grades[1].detail, "not addressed: where");
        assert_eq!(b.grades[1].detail, "not addressed: where");
    }

    #[test]
    fn a_typed_sentence_inside_a_changed_sentence_is_not_kept() {
        // The sentence survives as words, but it now says the opposite.
        let requirements = req("Paid today, thanks.", &[], &[]);
        let report = run(&requirements, "Hi Alex,\n\nNot paid today, thanks.\n\nAlex");
        assert_eq!(report.passed(Check::Kept), Some(false));
        assert!(
            report.grades[0]
                .detail
                .contains("changed: \"paid today, thanks\""),
            "{:?}",
            report.grades[0]
        );
    }

    #[test]
    fn an_answer_is_a_whole_word_so_now_does_not_answer_no() {
        let answers = [Answer {
            question: "will Priya join",
            any_of: &["no", "not"],
        }];
        let requirements = req("", &answers, &[]);
        assert_eq!(
            run(&requirements, "3pm works; Priya will join now.").passed(Check::Answers),
            Some(false)
        );
        assert_eq!(
            run(&requirements, "3pm works. Priya can't make it, no.").passed(Check::Answers),
            Some(true)
        );
    }

    #[test]
    fn a_preamble_is_a_leftover_even_when_clean_up_would_keep_it() {
        // clean_draft keeps this line (it ends in a full stop, not a colon), so
        // only the raw output shows it.
        let requirements = req("", &[], &[]);
        let text = "Sure, here's a draft of your reply.\n\nHi Alex,\n\nFriday works.\n\nAlex";
        let report = grade(&requirements, &Output { raw: text, text });
        assert_eq!(report.passed(Check::Leftovers), Some(false));
        assert!(report.grades[5]
            .detail
            .contains("preamble: \"Sure, here's a draft of your reply.\""));
    }

    #[test]
    fn a_draft_reply_wording_is_commentary_about_the_reply() {
        let requirements = req("", &[], &[]);
        for line in ["Here is the draft reply.", "Here's your draft reply."] {
            let text = format!("{line}\n\nHi Alex,\n\nFriday works.\n\nAlex");
            assert_eq!(
                run(&requirements, &text).passed(Check::Leftovers),
                Some(false),
                "{line}"
            );
        }
    }

    #[test]
    fn a_draft_document_in_an_email_is_not_a_preamble() {
        let requirements = req("", &[], &[]);
        let text = "Sure, thanks for the draft roadmap. I will read it properly on Monday.\n\nAlex";
        assert_eq!(
            run(&requirements, text).passed(Check::Leftovers),
            Some(true)
        );
    }

    #[test]
    fn commentary_about_the_reply_is_a_preamble_however_it_is_worded() {
        let requirements = req("", &[], &[]);
        for line in [
            "Here is my reply:",
            "Sure, here's a draft.",
            "Here's the draft:",
        ] {
            let text = format!("{line}\n\nHi Alex,\n\nFriday works.\n\nAlex");
            assert_eq!(
                run(&requirements, &text).passed(Check::Leftovers),
                Some(false),
                "{line}"
            );
        }
    }

    #[test]
    fn ordinary_email_sentences_are_not_preambles() {
        // The shapes a broad phrase match would have flagged: a lead word, then
        // a phrase that also appears in email text.
        let requirements = req("", &[], &[]);
        for line in [
            "Sure, here is the deck you asked for.",
            "Here's the updated pricing sheet.",
            "Sure, the reply is attached.",
            "Below is the agenda for Monday.",
            "Okayish, here's a draft.",
            "Sure, thanks for the draft roadmap. I will read it properly on Monday.",
        ] {
            let text = format!("{line}\n\nAlex");
            assert_eq!(
                run(&requirements, &text).passed(Check::Leftovers),
                Some(true),
                "{line}"
            );
        }
    }

    #[test]
    fn a_draft_reply_or_curly_apostrophe_commentary_is_a_preamble() {
        let requirements = req("", &[], &[]);
        for line in ["Here’s your draft reply.", "Below is the draft reply:"] {
            let text = format!("{line}\n\nHi Alex,\n\nFriday works.\n\nAlex");
            assert_eq!(
                run(&requirements, &text).passed(Check::Leftovers),
                Some(false),
                "{line}"
            );
        }
    }

    #[test]
    fn a_real_email_that_opens_with_sure_is_not_a_preamble() {
        let requirements = req("", &[], &[]);
        assert_eq!(
            run(&requirements, "Sure, Friday works for me.\n\nAlex").passed(Check::Leftovers),
            Some(true)
        );
    }
}
