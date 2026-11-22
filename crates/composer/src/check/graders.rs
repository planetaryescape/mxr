//! The six graders. Each reads a case's requirements and an output and
//! returns one grade that names what was required and what happened.

use super::text::{
    clauses, contains_phrase, copied_run, gap_texts, invented_numbers, sentences, words, Boundary,
    Clause,
};
use super::{Answer, AnswerPolarity, Check, Grade, Output, Requirements};
use std::collections::HashSet;
use std::ops::Range;

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
    let draft_clauses = clauses(output.text);
    let subjects: Vec<&str> = req
        .answers
        .iter()
        .flat_map(|answer| answer.subject.iter().copied())
        .collect();
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
        .filter(|answer| !is_answered(answer, &draft_words, &draft_clauses, &subjects))
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

/// Whether the draft gives this answer. An answer with a subject is read
/// only where that subject is talked about, so a "not" about the time cannot
/// answer a question about Priya. `subjects` are every answer's subjects in
/// the case.
fn is_answered(
    answer: &Answer<'_>,
    draft_words: &[String],
    draft_clauses: &[Clause],
    subjects: &[&str],
) -> bool {
    if answer.subject.is_empty() && answer.polarity == AnswerPolarity::Any {
        return answer
            .any_of
            .iter()
            .any(|phrase| contains_phrase(draft_words, phrase));
    }
    draft_clauses
        .iter()
        .enumerate()
        .filter(|(_, clause)| holds_subject(clause, answer.subject))
        .any(|(index, _)| {
            // "Priya will join, but she can't stay long" says yes: the
            // "can't" is about staying, so no phrase near Priya makes it a no.
            if answer.polarity == AnswerPolarity::Negative && says_yes(draft_clauses, index, answer)
            {
                return false;
            }
            let scope = &draft_clauses[answer_scope(draft_clauses, index, answer, subjects)];
            let literal: Vec<&[String]> = scope.iter().map(|c| c.words.as_slice()).collect();
            let without: Vec<Vec<String>> =
                scope.iter().map(|c| without_idioms(&c.words)).collect();
            // "no problem" can be the positive answer itself, but it never
            // supplies the "no" of a negative one.
            let stated = |phrase: &&str| match answer.polarity {
                AnswerPolarity::Negative => without.iter().any(|w| contains_phrase(w, phrase)),
                _ => literal.iter().any(|w| contains_phrase(w, phrase)),
            };
            answer.any_of.iter().any(stated)
                && (answer.polarity != AnswerPolarity::Positive
                    || !without.iter().any(|w| has_negation(w)))
        })
}

/// Whether the subject's own statement makes the question's claim with no
/// negation: "Priya will join" and "Priya will sort it out and join" say yes
/// to "will Priya join".
fn says_yes(clauses: &[Clause], index: usize, answer: &Answer<'_>) -> bool {
    let Some(verb) = question_verb(answer) else {
        return false;
    };
    let mut end = subject_clause_end(clauses, index, answer.subject);
    while clauses.get(end).is_some_and(|next| {
        next.opened_by == Boundary::And && continuation_kind(next) != Continuation::OtherSubject
    }) {
        end += 1;
    }
    let statement: Vec<String> = clauses[index..end]
        .iter()
        .flat_map(|clause| clause.words.iter().cloned())
        .collect();
    let statement = without_idioms(&statement);
    !has_negation(&statement)
        && statement.iter().any(|word| {
            word.strip_prefix(verb.as_str())
                .is_some_and(|ending| matches!(ending, "" | "s" | "ed" | "ing"))
        })
}

/// The word after the subject in a yes/no question: "join" in "will Priya
/// join". English puts the claim's verb or adjective there once the
/// auxiliary moves to the front.
fn question_verb(answer: &Answer<'_>) -> Option<String> {
    let question = words(answer.question);
    answer.subject.iter().find_map(|subject| {
        let subject = words(subject);
        let at = question
            .windows(subject.len().max(1))
            .position(|run| run == subject.as_slice())?;
        question.get(at + subject.len()).cloned()
    })
}

fn holds_subject(clause: &Clause, subject: &[&str]) -> bool {
    subject.is_empty()
        || subject
            .iter()
            .any(|subject| contains_phrase(&clause.words, subject))
}

/// The clauses that speak about the subject found at `index`.
fn answer_scope(
    clauses: &[Clause],
    index: usize,
    answer: &Answer<'_>,
    subjects: &[&str],
) -> Range<usize> {
    let mut end = subject_clause_end(clauses, index, answer.subject);
    let mut start = index;
    if answer.polarity == AnswerPolarity::Negative {
        while clauses
            .get(end)
            .is_some_and(|next| opens_with_person_pronoun(next) || concludes_with_no(next))
        {
            end += 1;
        }
        if let Some(lead) = refusal_lead(clauses, index, subjects) {
            start = lead;
        }
    }
    start..end
}

/// Where the statement about the subject ends. A clause that stops before
/// saying anything ("Priya,", "Priya is,") carries on across commas until its
/// predicate arrives or another subject takes over.
fn subject_clause_end(clauses: &[Clause], index: usize, subject: &[&str]) -> usize {
    let clause = &clauses[index];
    let is_bare = is_bare_subject(clause, subject);
    let is_unfinished = clause.words.last().is_some_and(|word| is_auxiliary(word));
    let mut end = index + 1;
    if !is_bare && !is_unfinished {
        return end;
    }
    while let Some(next) = clauses.get(end) {
        let joins_subjects = is_bare && next.opened_by == Boundary::And;
        if !joins_subjects && next.opened_by != Boundary::Soft {
            break;
        }
        let kind = continuation_kind(next);
        if kind == Continuation::OtherSubject && !joins_subjects {
            break;
        }
        end += 1;
        if kind != Continuation::Aside {
            return end;
        }
    }
    // "Priya? No." puts the answer in the sentence after a bare subject, but
    // "Priya? Sam can't make it." is about someone else.
    if is_bare
        && end == index + 1
        && clause.opened_by == Boundary::Sentence
        && clauses.get(end).is_some_and(|next| {
            next.opened_by == Boundary::Sentence
                && continuation_kind(next) != Continuation::OtherSubject
        })
    {
        end += 1;
    }
    end
}

/// "Priya" or "3pm" with nothing said about it yet.
fn is_bare_subject(clause: &Clause, subject: &[&str]) -> bool {
    clause.words.iter().all(|word| {
        subject
            .iter()
            .any(|subject| contains_phrase(std::slice::from_ref(word), subject))
    })
}

/// What a clause does to the statement it follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Continuation {
    /// "sadly", "our lead", "who runs the migration": the statement goes on.
    Aside,
    /// "can't make it": the statement ends here.
    Predicate,
    /// "Sam can't make it": the clause is about someone else.
    OtherSubject,
}

fn continuation_kind(clause: &Clause) -> Continuation {
    let rest: Vec<&String> = clause
        .words
        .iter()
        .skip_while(|word| word.ends_with("ly"))
        .collect();
    let says_something =
        |word: &String| is_auxiliary(word) || has_negation(std::slice::from_ref(word));
    match rest.split_first() {
        None => Continuation::Aside,
        Some((first, _)) if says_something(first) => Continuation::Predicate,
        Some((first, _)) if matches!(first.as_str(), "who" | "whom" | "whose" | "which") => {
            Continuation::Aside
        }
        Some(_) if is_short_refusal(&clause.words) => Continuation::Predicate,
        Some((_, others)) if others.iter().any(|word| says_something(word)) => {
            Continuation::OtherSubject
        }
        Some(_) => Continuation::Aside,
    }
}

fn is_auxiliary(word: &str) -> bool {
    matches!(
        word,
        "am" | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "been"
            | "will"
            | "would"
            | "can"
            | "could"
            | "shall"
            | "should"
            | "may"
            | "might"
            | "must"
            | "do"
            | "does"
            | "did"
            | "has"
            | "have"
            | "had"
    )
}

/// "No", "Sadly not", "Afraid not".
fn is_short_refusal(clause_words: &[String]) -> bool {
    clause_words.len() <= 3 && matches!(clause_words.last().map(String::as_str), Some("no" | "not"))
}

/// "I asked Priya, but she can't make it": the pronoun carries the subject on.
fn opens_with_person_pronoun(clause: &Clause) -> bool {
    clause
        .words
        .first()
        .is_some_and(|word| matches!(word.split('\'').next(), Some("she" | "he" | "they")))
}

/// "Priya is unavailable, so no": the conclusion belongs to the clause before.
fn concludes_with_no(clause: &Clause) -> bool {
    clause.opened_by == Boundary::Hard && clause.words == ["no"]
}

/// Where a short refusal that leads into the subject at `index` starts.
/// "No, Priya is travelling" and "Sadly not, Priya is away" lead with the
/// answer and name the subject after it; so does a refusal sentence of its
/// own, "No. Priya is travelling", unless it follows a bare subject: in
/// "3pm? No." the "No" answers 3pm.
fn refusal_lead(clauses: &[Clause], index: usize, subjects: &[&str]) -> Option<usize> {
    let opened_by = clauses[index].opened_by;
    if index == 0 || !matches!(opened_by, Boundary::Soft | Boundary::Sentence) {
        return None;
    }
    let lead = sentence_start(clauses, index - 1);
    let lead_words: Vec<String> = clauses[lead..index]
        .iter()
        .flat_map(|clause| clause.words.iter().cloned())
        .collect();
    if !is_short_refusal(&lead_words) {
        return None;
    }
    let answers_a_bare_subject = opened_by == Boundary::Sentence
        && lead > 0
        && clauses[lead - 1].opened_by == Boundary::Sentence
        && is_bare_subject(&clauses[lead - 1], subjects);
    (!answers_a_bare_subject).then_some(lead)
}

/// The first clause of the sentence that holds the clause at `index`.
fn sentence_start(clauses: &[Clause], index: usize) -> usize {
    (0..=index)
        .rev()
        .find(|&at| clauses[at].opened_by == Boundary::Sentence)
        .unwrap_or(0)
}

/// Phrases that use a negative word to say something positive.
const POSITIVE_IDIOMS: [&str; 17] = [
    "no problem",
    "no problems",
    "not a problem",
    "isn't a problem",
    "no issue",
    "no issues",
    "not an issue",
    "isn't an issue",
    "no worries",
    "no doubt",
    "no trouble",
    "don't mind",
    "can't wait",
    "won't miss",
    "wouldn't miss",
    "not miss",
    "not bad",
];

fn without_idioms(clause_words: &[String]) -> Vec<String> {
    let mut kept = clause_words.to_vec();
    for idiom in POSITIVE_IDIOMS {
        let idiom = words(idiom);
        while let Some(at) = kept
            .windows(idiom.len())
            .position(|run| run == idiom.as_slice())
        {
            kept.drain(at..at + idiom.len());
        }
    }
    kept
}

fn has_negation(clause_words: &[String]) -> bool {
    const NEGATIONS: [&str; 17] = [
        "no",
        "not",
        "never",
        "cannot",
        "can't",
        "won't",
        "wouldn't",
        "shouldn't",
        "couldn't",
        "don't",
        "doesn't",
        "didn't",
        "isn't",
        "aren't",
        "wasn't",
        "weren't",
        "unable",
    ];
    NEGATIONS
        .iter()
        .any(|negation| contains_phrase(clause_words, negation))
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
            subject: &[],
            polarity: AnswerPolarity::Any,
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
            subject: &[],
            polarity: AnswerPolarity::Any,
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
            subject: &[],
            polarity: AnswerPolarity::Any,
        };
        let second = Answer {
            question: "where",
            any_of: &["room 4"],
            subject: &[],
            polarity: AnswerPolarity::Any,
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
            subject: &[],
            polarity: AnswerPolarity::Any,
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
    fn answers_are_bound_to_their_own_question_and_polarity() {
        let answers = [
            Answer {
                question: "is 3pm fine",
                any_of: &["3pm"],
                subject: &["3pm"],
                polarity: AnswerPolarity::Positive,
            },
            Answer {
                question: "will Priya join",
                any_of: &["no", "not", "can't", "cannot", "won't", "out"],
                subject: &["Priya"],
                polarity: AnswerPolarity::Negative,
            },
        ];
        let requirements = req("", &answers, &[]);
        let wrong_subject_and_polarity = run(&requirements, "3pm does not work. Priya will join.");
        assert_eq!(
            wrong_subject_and_polarity.passed(Check::Answers),
            Some(false),
            "{wrong_subject_and_polarity:?}"
        );
        assert_eq!(
            run(&requirements, "3pm works. Priya cannot make it.").passed(Check::Answers),
            Some(true)
        );
        assert_eq!(
            run(&requirements, "Priya can’t make it. 3pm works!").passed(Check::Answers),
            Some(true),
            "answer order and punctuation do not change the result"
        );
    }

    const CALL_ANSWERS: [Answer<'static>; 2] = [
        Answer {
            question: "is 3pm fine",
            any_of: &["3pm"],
            subject: &["3pm"],
            polarity: AnswerPolarity::Positive,
        },
        Answer {
            question: "will Priya join",
            any_of: &[
                "no", "not", "can't", "cannot", "won't", "out", "isn't", "unable",
            ],
            subject: &["Priya"],
            polarity: AnswerPolarity::Negative,
        },
    ];

    #[test]
    fn a_comma_aside_keeps_the_answer_on_its_subject() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works. Priya is, unfortunately, not able to join.",
            "3pm works. Priya, however, can't make it.",
            "3pm works. Priya, our lead, can't make it.",
            "3pm works. Priya? Afraid not.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(true),
                "{draft}"
            );
        }
        let refused = run(
            &requirements,
            "3pm is, unfortunately, not possible. Priya is out.",
        );
        assert_eq!(refused.passed(Check::Answers), Some(false), "{refused:?}");
    }

    #[test]
    fn a_comma_continuation_with_another_subject_ends_the_answer() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works, but Priya can't make it.",
            "3pm works, Priya can't make it.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(true),
                "{draft}"
            );
        }
        for draft in [
            "3pm works. Priya is, sadly, Sam can't make it.",
            "3pm works. Priya, Sam can't make it.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(false),
                "{draft}"
            );
        }
    }

    #[test]
    fn an_answer_is_not_borrowed_from_another_subjects_sentence() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        assert_eq!(
            run(&requirements, "3pm works. Priya? Sam can't make it.").passed(Check::Answers),
            Some(false)
        );
        assert_eq!(
            run(&requirements, "3pm works. Priya? No.").passed(Check::Answers),
            Some(true)
        );
    }

    #[test]
    fn a_positive_answer_may_be_worded_as_an_idiom() {
        let answers = [Answer {
            question: "is 3pm fine",
            any_of: &["no problem"],
            subject: &["3pm"],
            polarity: AnswerPolarity::Positive,
        }];
        let requirements = req("", &answers, &[]);
        assert_eq!(
            run(&requirements, "3pm is no problem.").passed(Check::Answers),
            Some(true)
        );
        assert_eq!(
            run(&requirements, "3pm is not possible, no problem.").passed(Check::Answers),
            Some(false)
        );
        let negative = req("", &CALL_ANSWERS, &[]);
        assert_eq!(
            run(&negative, "3pm works. Priya can't wait to join.").passed(Check::Answers),
            Some(false)
        );
    }

    #[test]
    fn a_negation_in_another_clause_does_not_reverse_a_positive_answer() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works, but Priya can't make it.",
            "3pm works; Priya can't make it.",
            "3pm works and Priya can't make it.",
            "3pm works \u{2014} Priya can't make it.",
            "Priya can't make it, but 3pm works.",
            "3pm is no problem. Priya is out.",
            "I don't mind 3pm. Priya is out.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(true),
                "{draft}"
            );
        }
    }

    #[test]
    fn a_negation_in_the_subjects_own_clause_fails_a_positive_answer() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm can't work, but Priya is out.",
            "3pm, sadly, doesn't work. Priya can't make it.",
            "3pm? No. Priya can't make it.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(false),
                "{draft}"
            );
        }
    }

    #[test]
    fn a_negative_answer_needs_its_negation_on_the_subject() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works. Priya will join, no problem.",
            "3pm works. No problem, Priya will join.",
            "3pm works. Priya will join no problem.",
            "3pm works, not a problem, and Priya will join.",
            "3pm works. Priya can't wait to join.",
            "3pm works. Priya won't miss it.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(false),
                "{draft}"
            );
        }
    }

    #[test]
    fn a_negative_answer_may_reach_its_subject_in_the_ways_people_write_it() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works. Priya, sadly, can't make it.",
            "3pm works. Priya's out that day.",
            "3pm works. Priya and Sam can't make it.",
            "3pm works. I asked Priya, but she can't make it.",
            "3pm works. I asked Priya. She can't make it.",
            "3pm works. No, Priya is travelling.",
            "3pm works. Sadly not, Priya is away.",
            "3pm works. Priya is away, so she won't join.",
            "3pm works. Priya is unavailable, so no.",
            "3pm? Yes. Priya? No.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(true),
                "{draft}"
            );
        }
    }

    #[test]
    fn a_yes_about_the_subject_is_not_turned_into_a_no_by_another_negation() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works. Priya will join, but she can't stay long.",
            "3pm works. Priya will join. She can't stay long.",
            "3pm works. Priya will sort it out and join.",
            "3pm works. Priya will not miss it.",
            "3pm works. Priya is joining, but she can't stay long.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(false),
                "{draft}"
            );
        }
    }

    #[test]
    fn a_no_in_its_own_sentence_leads_into_the_subject_after_it() {
        let requirements = req("", &CALL_ANSWERS, &[]);
        for draft in [
            "3pm works. No. Priya is travelling.",
            "3pm works. Unfortunately, no. Priya is away.",
        ] {
            assert_eq!(
                run(&requirements, draft).passed(Check::Answers),
                Some(true),
                "{draft}"
            );
        }
        // After a bare "3pm?" the "No." answers 3pm, so Priya is unanswered.
        let report = run(&requirements, "3pm? No. Priya is travelling.");
        let grade = report
            .grades
            .iter()
            .find(|grade| grade.check == Check::Answers)
            .expect("answers grade");
        assert_eq!(grade.detail, "not addressed: is 3pm fine, will Priya join");
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
}
